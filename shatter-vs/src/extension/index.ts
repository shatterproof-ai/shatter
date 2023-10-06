import * as fs from 'fs'; //TODO: use VSCode fs
import * as path from 'path';
import { join } from 'path';
import * as ts from 'typescript';
import * as vscode from 'vscode';
import { AbsolutePath, RelativePath, Specimen, SpecimenId, isRelativePath, isSpecimenId, joinAbsolute } from '../core/common';
import { AutotestResults, ResultCluster, shatterAutotest } from '../core/shatter';
import { Outcome, RunResult } from '../core/supervisor';
import { FunctionMeta, findFunctions } from '../core/transform';

interface CommonDisplayNode {
	label: string;
	children?: CommonDisplayNode[];
	key?: string,
	state?: string,
	contextValue?: string,
}

function loadPersistedSpecimen(filepath: AbsolutePath) {
	const contents = fs.readFileSync(filepath, 'utf8');
	const specimen: Specimen = JSON.parse(contents);
	return specimen;
}

function loadPersistedSpecimens(workspaceRoot: AbsolutePath, specimenDirectory: AbsolutePath) {
	const suffix = '.json';
	const targetSubdirNames = ['custom', 'autotest'];
	const specimens: Map<SpecimenId, Specimental> = new Map();

	function traverseDirectory(directory: AbsolutePath, targetSubdirName: string) {
		if (!fs.existsSync(directory)) {
			return;
		}

		const files = fs.readdirSync(directory);
		for (const file of files) {
			const fullPath = join(directory, file) as AbsolutePath;
			const stats = fs.statSync(fullPath);
			if (stats.isDirectory()) {
				if (file === targetSubdirName) {
					const targetSubdirContents = fs.readdirSync(fullPath);
					for (const leafFile of targetSubdirContents) {
						if (leafFile.endsWith(suffix)) {
							const specimenId = leafFile.slice(0, -suffix.length);
							if (isSpecimenId(specimenId)) {
								const specimenPath = join(fullPath, leafFile) as AbsolutePath;
								const specimen = loadPersistedSpecimen(specimenPath);
								specimens.set(specimenId, {
									fileUnderTest: asAbsolutePath(workspaceRoot, specimen.fileUnderTest),
									specimenPath,
									specimen,
								});
								//	TODO: load corresponding results
							}
						}
					}
				} else {
					traverseDirectory(fullPath, targetSubdirName);
				}
			}
		}
	}

	for (const targetSubdirName of targetSubdirNames) {
		traverseDirectory(specimenDirectory, targetSubdirName);
	}
	return specimens;
}

function saveTest(specimenBaseDirectory: AbsolutePath, specimental: Specimental, result?: RunResult) {
	//	TODO: don't save everything from a specimen, notably omit the leaves and any parentage
	const specimenSubdir = specimental.specimen.id.startsWith('custom') ? 'custom' : 'autotest';
	const specimenFileAbsolutePath = join(specimenBaseDirectory, 'specimens', specimenSubdir, `${specimental.specimen.id}.json`) as AbsolutePath;
	const specimenSubdirectory = path.dirname(specimenFileAbsolutePath);
	fs.mkdirSync(specimenSubdirectory, { recursive: true });
	fs.writeFileSync(specimenFileAbsolutePath, JSON.stringify(specimental.specimen, undefined, 2));

	if (result) {
		const resultsFileAbsolute = join(specimenBaseDirectory, 'results', specimenSubdir, `${specimental.specimen.id}.json`) as AbsolutePath;
		const resultsDirectory = path.dirname(resultsFileAbsolute);
		fs.mkdirSync(resultsDirectory, { recursive: true });
		fs.writeFileSync(resultsFileAbsolute, JSON.stringify(result, undefined, 2));
	}

	return specimenFileAbsolutePath;
}

function forkTest(specimenBaseDirectory: AbsolutePath, original: Specimental, newId: SpecimenId, name: string,) {
	const newSpeciment: Specimen = {
		...original.specimen,
		type: 'custom',
		id: newId,
		name,
	};
	const newSpecimental = {
		...original,
		specimen: newSpeciment,
	};

	const specimenFileAbsolutePath = saveTest(specimenBaseDirectory, newSpecimental);
	newSpecimental.specimenPath = specimenFileAbsolutePath;
	return newSpecimental;
}

function visit(k: string | number, o: any, depth = 0): CommonDisplayNode {
	if (depth === 0) {
		return {
			label: "...",
		};
	}

	const key = typeof k === 'number' ? `[${k}]` : `"${k}"`;
	if (o === null) {
		return {
			label: `${key}: null`,
		};
	}
	if (o === undefined) {
		return {
			label: `${key}: undefined`,
		};
	}
	if (typeof o === 'object') {
		if (Array.isArray(o)) {
			return {
				label: key,
				children: o.map((v, i) => visit(i, v, depth - 1)),
			};
		}
		const keys = Object.keys(o);
		const children = keys.map((k) => visit(k, o[k], depth - 1));
		return {
			label: key,
			children,
		};
	}

	return {
		label: `${key}: ${JSON.stringify(o)}`,
	};
}

type FunctionState = {
	autotest: AutotestResults;
	specimens: Record<string, Specimental>;	//	Record because Map is not serializable
};

type FileState = {
	functions: FunctionMeta[];
	functionStates: Record<string, FunctionState>;	//	Record because Map is not serializable
};

type CoverageSelection = 'all'
	| 'missed'
	| { clusterKeys: string[] };

interface Specimental {
	fileUnderTest: AbsolutePath,
	specimenPath?: AbsolutePath,			//	empty if not persisted
	clusterKey?: string,	//	empty if never run
	specimen: Specimen,
}

interface ExtensionState {
	runningAutotestFunction?: string;
	fileStates: Record<AbsolutePath, FileState>;	//	Record because Map is not serializable
	//	this overlaps some with specimens, but it doesn't load the contents	
	activeFile?: AbsolutePath;
	activeFunction?: string;
	activeCoverage?: CoverageSelection;
	activeSpecimenId?: string;
};

interface Providers {
	functionsListProvider: CommonTreeDataProvider,
	clustersListProvider: CommonTreeDataProvider,
	testCaseListProvider: CommonTreeDataProvider,
	testCaseDetailProvider: CommonTreeDataProvider,
}

const coveredDecorationType = vscode.window.createTextEditorDecorationType({
	// gutterIconPath: context.asAbsolutePath('media/triangle.svg'),
	//	TODO: get colors from theme and/or IDE https://code.visualstudio.com/api/references/theme-color#text-colors
	light: {
		backgroundColor: 'lightblue',
	},
	dark: {
		backgroundColor: 'midnightblue',
	},
});

const missedDecorationType = vscode.window.createTextEditorDecorationType({
	// gutterIconPath: context.asAbsolutePath('media/triangle.svg'),
	//	TODO: get colors from theme and/or IDE https://code.visualstudio.com/api/references/theme-color#text-colors
	light: {
		backgroundColor: 'orange',
	},
	dark: {
		backgroundColor: 'maroon',
	},
});

function resetDecorations(editor: vscode.TextEditor) {
	editor.setDecorations(coveredDecorationType, []);
	editor.setDecorations(missedDecorationType, []);
}

function asAbsolutePath(workspaceRoot: AbsolutePath, filename: RelativePath): AbsolutePath {
	return join(workspaceRoot, filename) as AbsolutePath;
}

function asRelativePath(filename: AbsolutePath): RelativePath | undefined {
	if (!vscode.workspace.workspaceFolders) {
		return;
	}

	const fileUri = vscode.Uri.from({ scheme: 'file', path: filename });
	for (const wsf of vscode.workspace.workspaceFolders) {
		if (fileUri.fsPath.startsWith(wsf.uri.fsPath)) {
			return vscode.workspace.asRelativePath(filename) as RelativePath;
		}
	}
	return;
}

function getActiveStates(extensionState: ExtensionState): {
	fileState?: FileState,
	functionState?: FunctionState,
	functionMeta?: FunctionMeta,
	specimental?: Specimental,
} {
	const activeFilename = extensionState.activeFile;
	if (!activeFilename) {
		//	TODO: clear functions list, clusters list, branches list, test cases list
		return {};
	}

	const fileState = extensionState.fileStates[activeFilename];
	if (!fileState || !fileState.functions) {
		//	TODO: clear what needs clearing
		return {};
	}

	const activeFunction = extensionState.activeFunction;
	if (!activeFunction) {
		return { fileState };
	}

	const functionMeta = fileState.functions.find((f) => f.name === activeFunction);
	if (!functionMeta) {
		//	this is not necessarily an error because the function may have been deleted
		return { fileState };
	}

	const functionState = fileState.functionStates[activeFunction];

	const activeSpecimenId = extensionState.activeSpecimenId;
	if (!activeSpecimenId) {
		return { fileState, functionState, functionMeta };
	}

	const specimental = functionState.specimens[activeSpecimenId];
	if (!specimental) {
		return { fileState, functionState, functionMeta };
	}

	return { fileState, functionState, functionMeta, specimental };
}

const refresh = (editor: vscode.TextEditor | undefined, extensionState: ExtensionState, providers: Providers) => {
	const { functionsListProvider, clustersListProvider, testCaseListProvider, testCaseDetailProvider } = providers;

	const { fileState, functionState, functionMeta, specimental } = getActiveStates(extensionState);

	if (!fileState) {
		functionsListProvider.refresh([]);
		clustersListProvider.refresh([]);
		testCaseListProvider.refresh([]);
		testCaseDetailProvider.refresh([]);
		return;
	}

	const nodes: CommonDisplayNode[] = fileState.functions.map((f) => {
		const runningTest = extensionState.runningAutotestFunction === f.name;
		const runningTestLabel = runningTest ? " - testing now" : "";
		return {
			label: `${f.name}${runningTestLabel}` || "",
			key: f.name || "",
		};
	});

	functionsListProvider.refresh(nodes);

	// if (!functionState) {
	// console.log(`nonono results for filename "${filename}" and function "${extensionState.activeFunction}" - ${JSON.stringify(fileState.functionStates)}`)
	// return;
	// };

	if (!functionState || !functionMeta) {
		clustersListProvider.refresh([]);
		testCaseListProvider.refresh([]);
		testCaseDetailProvider.refresh([]);
		return;
	}

	const activeCoverage = extensionState.activeCoverage;

	const results = functionState.autotest;
	const selectedClusters: ResultCluster[] = (results.clusters ?? [])
		.filter(c => activeCoverage === undefined
			|| activeCoverage === 'all'
			|| (activeCoverage !== 'missed' && activeCoverage.clusterKeys.includes(c.key)));

	if (results) {
		const nodesByOutcome: Record<Outcome, CommonDisplayNode[]> = {
			completed: [],
			error: [],
			timeout: [],
			failed: []
		};

		const countByOutcome: Record<Outcome, number> = {
			completed: 0,
			error: 0,
			timeout: 0,
			failed: 0
		};

		const linesByOutcome: Record<Outcome, Set<number>> = {
			error: new Set(),
			completed: new Set(),
			timeout: new Set(),
			failed: new Set(),
		};

		const functionInstrumentedLines = new Set<number>();
		for (let line = functionMeta.startLine; line <= functionMeta.endLine; line++) {
			if (functionState.autotest.instrumentedLines.includes(line)) {
				functionInstrumentedLines.add(line);
			}
		}

		const formatter = Intl.NumberFormat("en-US", { style: "percent" });
		//	TODO: sort by coverage
		selectedClusters.forEach((cluster) => {
			const key = cluster.key.substring(0, 6);
			countByOutcome[cluster.outcome] += cluster.results.length;
			cluster.lines.forEach(line => linesByOutcome[cluster.outcome].add(line));
			nodesByOutcome[cluster.outcome].push({
				//	TODO: skip coverage for timeouts and failures
				label: `${key} - ${formatter.format(cluster.lines.length / functionInstrumentedLines.size)} coverage (${cluster.results.length} test cases)`,
				key: `cluster://${cluster.key}`,
			});
		});

		const capitalize = (s: string) => {
			return s.charAt(0).toUpperCase() + s.slice(1);
		};

		const clusterNodes: CommonDisplayNode[] = Object.entries(nodesByOutcome)
			.map(([outcome, nodes]) => {
				const baseLabel = capitalize(outcome);
				const coverageText = (() => {
					if (outcome === 'timeout' || outcome === 'failed') {
						return "";
					}
					const coverage = linesByOutcome[outcome as Outcome].size / functionInstrumentedLines.size;
					return `- ${formatter.format(coverage)} coverage `;
				})();

				return {
					label: `${baseLabel} ${coverageText}(${countByOutcome[outcome as Outcome] ?? 0} test case(s))`,
					children: nodes,
				};
			});

		const allCoveredLines = new Set<number>();
		Object.values(linesByOutcome).forEach((lines) => {
			lines.forEach((line) => allCoveredLines.add(line));
		});
		const totalCoverageFraction = allCoveredLines.size / functionInstrumentedLines.size;
		const uncoveredFraction = 1 - totalCoverageFraction;
		clusterNodes.push({
			label: `Not covered ${formatter.format(uncoveredFraction)} (${functionInstrumentedLines.size - allCoveredLines.size} lines)`,
			key: "missed://",
		});

		clustersListProvider.refresh(clusterNodes);
		if (editor) {
			resetDecorations(editor);
			const covered = new Set(selectedClusters.flatMap((cluster) => cluster.lines));
			const lines = (() => {
				if (activeCoverage === 'missed') {
					const uncovered = Array.from(functionInstrumentedLines)
						.filter((line) => !covered.has(line))
						.sort((a, b) => a - b);
					return uncovered;
				}
				return Array.from(covered).sort((a, b) => a - b);
			})();

			function* linerator() {
				for (const line of lines ?? []) {
					yield line;
				}
			}

			const decorationType = activeCoverage === 'missed' ? missedDecorationType : coveredDecorationType;

			//	TODO: replace with function pointer or pubsub or something that doesn't require passing around the editor object
			highlightLinesInEditor(editor, decorationType, linerator());
		}
	}

	const shortString = (a: any) => {
		if (a === null) {
			return 'null';
		}
		if (a === undefined) {
			return 'undefined';
		}
		const s = typeof a === 'string' ? a : JSON.stringify(a);
		const maxLength = 40;

		const strung = (s.length > maxLength)
			? s.substring(0, maxLength - 3) + '...'
			: s;
		return strung;
	};

	if (activeCoverage === 'missed') {
		testCaseListProvider.refresh([]);
		testCaseDetailProvider.refresh([]);
		return;
	}

	const testCaseListNodes: CommonDisplayNode[] = selectedClusters
		.filter(c => activeCoverage === undefined
			|| activeCoverage === 'all'
			|| activeCoverage.clusterKeys.includes(c.key))
		.flatMap(c => c.results.map((result, i): CommonDisplayNode => {
			const specimental = functionState.specimens[result.specimenId];

			const contextPieces: string[] = [];

			if (specimental?.specimen?.id.startsWith('custom')) {
				contextPieces.push('custom');
			} else {
				contextPieces.push('autotest');
			}

			if (specimental?.specimenPath) {
				contextPieces.push('persistent');
			}

			const state = specimental?.specimenPath ? 'pinned' : 'unpinned';
			const parametersNode = {
				label: shortString(result.serializedParameterValues),
				key: result.specimenId,
				state,
				contextValue: contextPieces.join(','),
			};
			return parametersNode;
		}));
	testCaseListProvider.refresh(testCaseListNodes);

	if (!specimental) {
		return;
	}

	const result = (() => {
		for (const cluster of selectedClusters) {
			const result = cluster.results.find(c => c.specimenId === specimental.specimen.id);
			if (result) {
				return result;
			}
		}
	})();

	if (!result) {
		return;
	}

	//	TODO: make this cleaner, ideally like JSON.stringify(...)
	const metadataNode = {
		label: `Duration ${result.duration}ms`
	};
	const resultNode = visit('Result', result.output ?? result.error, 3);

	if (!specimental.specimen) {
		console.error(`Unable to find specimen ${result.specimenId}`);
		return;
	}

	const parametersNode = visit('Parameters', specimental.specimen.parameters, 3);
	const testCaseNodes: CommonDisplayNode[] = [
		metadataNode,
		parametersNode,
		resultNode,
	];

	testCaseDetailProvider.refresh(testCaseNodes);
};

const doSelectFunction = (editor: vscode.TextEditor, extensionState: ExtensionState, providers: Providers, functionName: string) => {
	if (!extensionState.activeFile) {
		//	TODO: shouldn't happen
		return;
	}
	const filename = extensionState.activeFile;
	const filestate = extensionState.fileStates[filename];
	if (!filestate) {
		//	TODO: shouldn't happen; TODO: can regenerate
		return;
	}

	const selectedFunction = filestate.functions.find((f) => f.name === functionName);
	if (selectedFunction) {
		extensionState.activeFunction = functionName;
	} else {
		extensionState.activeCoverage = undefined;
		extensionState.activeFunction = undefined;
	}
	refresh(editor, extensionState, providers);
};

const doSelectCluster = (editor: vscode.TextEditor, context: vscode.ExtensionContext, extensionState: ExtensionState, providers: Providers,
	coverage: CoverageSelection) => {
	if (!extensionState.activeFile) {
		//	TODO: shouldn't happen
		return;
	}
	const filename = extensionState.activeFile;
	const filestate = extensionState.fileStates[filename];
	if (!filestate) {
		//	TODO: shouldn't happen
		return;
	}

	if (!extensionState.activeFunction) {
		return;
	}

	const functions = findFunctions(filename);

	const selectedFunction = functions.find((f) => f.name === extensionState.activeFunction);
	if (!selectedFunction) {
		//	TODO: shouldn't happen
		return;
	}

	const functionState = filestate.functionStates[extensionState.activeFunction];
	if (!functionState) {
		//	TODO: shouldn't happen
		return;
	}

	extensionState.activeCoverage = coverage;
	refresh(editor, extensionState, providers);
};

const doSelectTestCase = (editor: vscode.TextEditor, context: vscode.ExtensionContext, extensionState: ExtensionState, providers: Providers,
	specimenId: string, conf?: ProjectConfiguration) => {
	if (!extensionState.activeFile) {
		return;
	}

	const filename = extensionState.activeFile;
	const filestate = extensionState.fileStates[filename];
	if (!filestate) {
		return;
	}

	if (!extensionState.activeFunction) {
		return;
	}

	const functions = findFunctions(filename);

	const selectedFunction = functions.find((f) => f.name === extensionState.activeFunction);
	if (!selectedFunction) {
		//	TODO: shouldn't happen
		return;
	}

	const functionState = filestate.functionStates[extensionState.activeFunction];
	if (!functionState) {
		//	TODO: shouldn't happen
		return;
	}

	extensionState.activeSpecimenId = specimenId;
	refresh(editor, extensionState, providers);
};

function highlightLinesInEditor(editor: vscode.TextEditor | undefined, decorationType: vscode.TextEditorDecorationType, liner: Generator<number, void, unknown>) {
	if (!editor) {
		return;
	}

	const decorationsArray: vscode.DecorationOptions[] = [];
	const lines: number[] = [];
	for (const lineNumber of liner) {
		if (lineNumber >= editor.document.lineCount) {
			break;
		}
		//	line numbers are ZERO based or ONE based?
		const line = editor.document.lineAt(lineNumber);
		//	TODO: collapse contiguous line numbers into a range
		const decoration = { range: line.range, hoverMessage: `Line ${lineNumber + 1}: ${line.text}` };
		decorationsArray.push(decoration);
		lines.push(lineNumber);
	};
	console.log(`highlightLinesInEditor ${JSON.stringify(lines)}}`);

	editor.setDecorations(decorationType, decorationsArray);
}

//	We never write this; it's populated only by reading, and it doesn't exactly
//	match the format on disk
interface ProjectConfiguration {
	testsDirectory?: RelativePath;	//	RELATIVE to the project root
	projectRootDirectory?: RelativePath;	//	RELATIVE to the project root
}

async function readProjectConfiguration(workspaceRoot: AbsolutePath): Promise<ProjectConfiguration> {

	const matches = await vscode.workspace.findFiles('shatterproof.json', '**/node_modules/**', 1);

	for (const fileUri of matches) {
		try {
			const fileStat = await vscode.workspace.fs.stat(fileUri);
			if (fileStat) {
				const relativePath = vscode.workspace.asRelativePath(fileUri);
				const pattern = new vscode.RelativePattern(path.dirname(relativePath), 'shatterproof.json');

				function loadConfiguration() {
					return vscode.workspace.fs.readFile(fileUri)
						.then((contentsInts) => {
							const contents = Buffer.from(contentsInts).toString('utf8');
							try {
								const pc = JSON.parse(contents);
								if (pc.testsDirectory) {
									if (isRelativePath(pc.testsDirectory)) {
										return {
											configuration: {
												testsDirectory: asAbsolutePath(workspaceRoot, pc.testsDirectory),
											}
										};
									}

									return {
										testsDirectory: pc.testsDirectory,
									};
								}
							} catch (e) {
								//	TODO: handle error
								const ee = e;
							}
							return {};
						});
				}

				const watcher = vscode.workspace.createFileSystemWatcher(pattern, false, false, false);

				//	DO NOT want to support this file changing at least while it only has the path for the tests directory
				// watcher.onDidChange(loadConfiguration);
				// watcher.onDidDelete(_ => { holder.configuration = undefined; });

				return loadConfiguration();
			}
		} catch (e) {
			//	throws an error if the file doesn't exist; there's no simple existence check
			/*
	EntryNotFound (FileSystemError): Error: ENOENT: no such file or directory, stat '/shatterproof.json'
		at k.e (/snap/code/141/usr/share/code/resources/app/out/vs/workbench/api/node/extensionHostProcess.js:109:26741)
		at Object.stat (/snap/code/141/usr/share/code/resources/app/out/vs/workbench/api/node/extensionHostProcess.js:109:24556)
		at async readProjectConfiguration (/home/ketan/project/shatter/shatter-vs/out/extension/index.js:382:26)
		at async activate (/home/ketan/project/shatter/shatter-vs/out/extension/index.js:433:18)
		at async E.n (/snap/code/141/usr/share/code/resources/app/out/vs/workbench/api/node/extensionHostProcess.js:107:6206)
		at async E.m (/snap/code/141/usr/share/code/resources/app/out/vs/workbench/api/node/extensionHostProcess.js:107:6169)
		at async E.l (/snap/code/141/usr/share/code/resources/app/out/vs/workbench/api/node/extensionHostProcess.js:107:5626) {code: 'FileNotFound', name: 'EntryNotFound (FileSystemError)', stack: 'EntryNotFound (FileSystemError): Error: ENOEN…ch/api/node/extensionHostProcess.js:107:5626)', message: 'Error: ENOENT: no such file or directory, stat '/shatterproof.json''}
			*/
			const ee = e;
		}
	}

	return {};
}

function editTestCase(workspaceRoot: AbsolutePath, filename: RelativePath, functionName: string, testCase: string) {
	const uri = vscode.Uri.file(filename);
	vscode.workspace.openTextDocument(uri)
		.then((doc) => {
			vscode.window.showTextDocument(doc)
				.then((editor) => {
					const functions = findFunctions(asAbsolutePath(workspaceRoot, filename));
					const selectedFunction = functions.find((f) => f.name === functionName);
					if (!selectedFunction) {
						return;
					}
				});
		});
}
/*
Operations:
* open test case
* save test case
* add test case

Provide context menu for running a test case from a file

TODO: convert the test case tree view into a test case  manager

How to select test cases?  Per function, per cluster, per test case

*/

//	this exists primarily for the situation where the ExtensionState that was
//	persisted has a different structure than what the code uses now
function cleanUpExtensionState(initial: Partial<ExtensionState>) {
	const fullExtensionState: ExtensionState = {
		fileStates: {},
		...initial,
	};

	if (!fullExtensionState.fileStates) {
		fullExtensionState.fileStates = {};
	}

	for (const [filename, fileState] of Object.entries(fullExtensionState.fileStates)) {
		if (! fileState.functions) {
			fileState.functions = [];
		}
		if (! fileState.functionStates) {
			fileState.functionStates = {};
		}
		for (const [functionName, functionState] of Object.entries(fileState.functionStates)) {
			if (!functionState.specimens) {
				//	at least once there was a failed serialization and the specimens property wasn't present
				functionState.specimens = {};
			}
		}
	}

	return fullExtensionState;
}

function onPersistedSpecimenLoad(defaultWorkspaceRoot: AbsolutePath, extensionState: ExtensionState, specimen: Specimen, maybeSpecimenId: string, absoluteSpecimenFilepath: AbsolutePath | undefined) {
	const absoluteSourceFilepath = asAbsolutePath(defaultWorkspaceRoot, specimen.fileUnderTest);
	if (!extensionState.fileStates[absoluteSourceFilepath]) {
		extensionState.fileStates[absoluteSourceFilepath] = {
			functions: [],
			functionStates: {},
		};
	}

	const fileState = extensionState.fileStates[absoluteSourceFilepath];
	if (!fileState.functionStates[specimen.functionName]) {
		fileState.functionStates[specimen.functionName] = {
			autotest: {
				clusters: [],
				instrumentedLines: [],
			},
			specimens: {},
		};
	}

	const functionState = fileState.functionStates[specimen.functionName];
	const existing = functionState.specimens[maybeSpecimenId];
	if (existing) {
		console.log(`Unexpectedly (?) found existing specimen ${maybeSpecimenId} for ${specimen.functionName} in ${absoluteSourceFilepath}`);
	}

	functionState.specimens[specimen.id] = {
		fileUnderTest: absoluteSourceFilepath,
		specimenPath: absoluteSpecimenFilepath,
		clusterKey: undefined,
		specimen,
	};
}

const autotestStorageStateKey = "autotestState_0";
export async function activate(context: vscode.ExtensionContext) {
	//	TODO: this all needs to deal in URIs
	const workspaceRoots: AbsolutePath[] = vscode.workspace.workspaceFolders?.map((f) => f.uri.fsPath as AbsolutePath) ?? [];
	const defaultWorkspaceRoot: AbsolutePath | undefined = workspaceRoots[0];
	let configuration: ProjectConfiguration = {};
	let specimenBaseDirectory: AbsolutePath | undefined = undefined;

	const extensionState: ExtensionState = cleanUpExtensionState(context.workspaceState.get(autotestStorageStateKey, {}));

	try {
		if (defaultWorkspaceRoot) {
			configuration = await readProjectConfiguration(defaultWorkspaceRoot);
			if (configuration.testsDirectory) {
				//	NOTE: this watcher is using a different API than listPersistedSpecimens because
				//	the latter is meant to be independent of VS Code
				const ignoreCreate = false;
				const ignoreChange = true;
				const ignoreDelete = false;
				const watcher = vscode.workspace.createFileSystemWatcher(`${configuration.testsDirectory}/**/*.json`, ignoreCreate, ignoreChange, ignoreDelete);

				watcher.onDidCreate((e) => {
					const absoluteSpecimenFilepath = e.fsPath as AbsolutePath;
					const maybeSpecimenId = path.basename(absoluteSpecimenFilepath).substring(0, '.json'.length);
					if (isSpecimenId(maybeSpecimenId)) {
						const specimen = loadPersistedSpecimen(absoluteSpecimenFilepath as AbsolutePath);

						if (!defaultWorkspaceRoot) {
							throw new Error(`Unexpectedly no workspace root for ${absoluteSpecimenFilepath}`);
						}

						onPersistedSpecimenLoad(defaultWorkspaceRoot, extensionState, specimen, maybeSpecimenId, absoluteSpecimenFilepath);
					}
				});

				watcher.onDidDelete((e) => {
					const filepath = e.fsPath;
					const maybeSpecimenId = path.basename(filepath).substring(0, '.json'.length);
					if (isSpecimenId(maybeSpecimenId)) {
						//	deleting the file means we should mark it not persistent
						for (const [absoluteFileName, fileState] of Object.entries(extensionState.fileStates)) {
							for (const [functionName, functionState] of Object.entries(fileState.functionStates)) {
								const specimen = functionState.specimens[maybeSpecimenId];
								if (specimen?.specimenPath === filepath) {
									delete functionState.specimens[maybeSpecimenId];
									return;
								}
							}
						}
					}
				});

				//	do this *after* the watcher is set up to avoid missing any additions
				//	TODO: might miss some deletions
				specimenBaseDirectory = asAbsolutePath(defaultWorkspaceRoot, configuration.testsDirectory);
				const initialPersistentSpecimens = loadPersistedSpecimens(defaultWorkspaceRoot, specimenBaseDirectory);
				initialPersistentSpecimens.forEach((specimental, id) => {
					onPersistedSpecimenLoad(defaultWorkspaceRoot, extensionState, specimental.specimen, id, specimental.specimenPath);
				});
			}
		}

		//	TODO: Refresh functions list view contents on change of editor
		const functionsListProvider = new CommonTreeDataProvider({
			command: {
				command: 'extension.shatterSelectFunction',
				title: 'Functions',
			}
		});
		context.subscriptions.push(
			vscode.window.registerTreeDataProvider("shatter-functions-list", functionsListProvider));

		const clustersListProvider = new CommonTreeDataProvider({
			command: {
				command: 'extension.shatterSelectCluster',
				title: 'Execution Paths',
			}
		});
		context.subscriptions.push(
			vscode.window.registerTreeDataProvider("shatter-execution-paths", clustersListProvider));

		function iconPaths(baseSet: Record<string, string>) {
			const expanded: Record<string, Record<'light' | 'dark', string>> = {};

			for (const [status, baseIconPath] of Object.entries(baseSet)) {
				const light = context.asAbsolutePath(`resources/light/${baseIconPath}`);
				const dark = context.asAbsolutePath(`resources/dark/${baseIconPath}`);
				expanded[status] = {
					light,
					dark,
				};
			}

			return expanded;
		}
		const testCaseListProvider = new CommonTreeDataProvider({
			command: {
				command: 'extension.shatterSelectTestCase',
				title: 'Test Case Detail',
			},
			stateIcons: iconPaths({ pinned: 'pin.svg', unpinned: 'unpin.svg' }),
		});
		context.subscriptions.push(
			vscode.window.registerTreeDataProvider("shatter-list-testcases", testCaseListProvider));

		const testCaseDetailProvider = new CommonTreeDataProvider({
			stateIcons: iconPaths({ persistent: 'pin.svg' }),
		});
		context.subscriptions.push(
			vscode.window.registerTreeDataProvider("shatter-testcase-detail", testCaseDetailProvider));

		const providers = {
			functionsListProvider,
			clustersListProvider,
			testCaseListProvider,
			testCaseDetailProvider,
		};

		const updateSelectedFile = () => {
			const filename = vscode.window.activeTextEditor?.document.fileName;
			if (!filename) {
				//	TODO: clear functions list
				return;
			}
			if (vscode.window.activeTextEditor?.document.languageId === 'typescript') {
				doSelectFile(vscode.window.activeTextEditor, context, extensionState, filename as AbsolutePath, providers);
			}
		};

		//	call after switching files, changing contents of the editor, or running tests
		const doSelectFunctionCommand = (node: CommonDisplayNode) => {
			if (vscode.window.activeTextEditor?.document.languageId === 'typescript') {
				const functionName: string = node.key || "";
				doSelectFunction(vscode.window.activeTextEditor, extensionState, providers, functionName);
			}
		};

		const doSelectClusterCommand = (node: CommonDisplayNode) => {
			if (vscode.window.activeTextEditor?.document.languageId === 'typescript') {
				const selection: CoverageSelection | undefined = (() => {
					if (node.key) {
						if (node.key.startsWith('cluster://')) {
							const clusterKey = node.key.substring('cluster://'.length);
							return { clusterKeys: [clusterKey] };
						}
						if (node.key === 'covered://') {
							return 'all';
						}
						if (node.key === 'missed://') {
							return 'missed';
						}
						throw new Error(`unhandled key ${node.key}`);
					}
				})();
				if (selection) {
					doSelectCluster(vscode.window.activeTextEditor, context, extensionState, providers, selection);
				}
			}
		};

		const doSelectTestCaseCommand = (node: CommonDisplayNode) => {
			if (vscode.window.activeTextEditor?.document.languageId === 'typescript') {
				const specimenId: string = node.key || "";
				doSelectTestCase(vscode.window.activeTextEditor, context, extensionState, providers, specimenId);
			}
		};

		//	needs to be registered as a command because TreeView needs a command to dispatch to
		const selectFunctionCommand = vscode.commands.registerCommand('extension.shatterSelectFunction', doSelectFunctionCommand);
		context.subscriptions.push(selectFunctionCommand);

		//	needs to be registered as a command because TreeView needs a command to dispatch to
		const selectClusterCommand = vscode.commands.registerCommand('extension.shatterSelectCluster', doSelectClusterCommand);
		context.subscriptions.push(selectClusterCommand);

		//	needs to be registered as a command because TreeView needs a command to dispatch to
		const selectTestCaseCommand = vscode.commands.registerCommand('extension.shatterSelectTestCase', doSelectTestCaseCommand);
		context.subscriptions.push(selectTestCaseCommand);

		/*
		
		generated test case:
		* user clicks pin - saves it to specified location (TODO: add it to the working tree)
		* user clicks unpin - deletes it from the specified location (TODO: remove it from the working tree)
		* user clicks edit - IF a non-custom test case, ask for a name, save it to the specified location, and open an editor for that file
		* user clicks add - ask for a name, create an empty file, open an editor
		
		TODO: Editor should be able to match parameter type structure with autocomplete and validation.  Custom language server based on function and signature?
		
		//	where to track test case persistence?
		
		*/

		const makeTestCasePersistentCommand = vscode.commands.registerCommand('extension.shatterMakeTestcasePersistent', async (node: CommonDisplayNode) => {
			//	if the test case is not persistent, save it to the location specified in the configuration
			const specimenId = node.key;
			if (!specimenBaseDirectory || !isSpecimenId(specimenId)) {
				return;
			}

			const { specimental } = getActiveStates(extensionState);
			if (!specimental || specimental.specimenPath) {	//	already persisted
				return;
			}

			const savePath = saveTest(specimenBaseDirectory, specimental);
			specimental.specimenPath = savePath;
			refresh(vscode.window.activeTextEditor, extensionState, providers);
		});
		context.subscriptions.push(makeTestCasePersistentCommand);

		const makeTestcaseNotPersistentCommand = vscode.commands.registerCommand('extension.shatterMakeTestcaseNonPersistent', async (node: CommonDisplayNode) => {
			const specimenId = node.key;
			if (!specimenBaseDirectory || !isSpecimenId(specimenId)) {
				return;
			}

			const { specimental } = getActiveStates(extensionState);
			if (!specimental || !specimental.specimenPath) {	//	already persisted
				return;
			}

			const fileUri = vscode.Uri.file(specimental.specimenPath);
			await vscode.workspace.fs.delete(fileUri);
			specimental.specimenPath = undefined;
			refresh(vscode.window.activeTextEditor, extensionState, providers);
		});
		context.subscriptions.push(makeTestcaseNotPersistentCommand);

		const editTestCaseCommand = vscode.commands.registerCommand('extension.shatterEditCustomTestcase', async (node: CommonDisplayNode) => {
			const specimenId = node.key;
			if (!specimenBaseDirectory || !specimenId) {
				return;
			}

			const specimenPath = ((): AbsolutePath | undefined => {
				//	if it's a generated specimen, fork to a custom specimen
				const { specimental } = getActiveStates(extensionState);
				if (!specimental) {
					return;
				}
				if (specimenId.startsWith('custom')) {
					return specimental.specimenPath;
				}
			})();

			if (specimenPath && vscode.window.activeTextEditor?.document.languageId === 'typescript') {
				if (fs.existsSync(specimenPath)) {
					vscode.workspace.openTextDocument(specimenPath).then((doc) => {
						vscode.window.showTextDocument(doc, vscode.ViewColumn.One);
					});
				} else {
					vscode.window.showErrorMessage(`Test case ${specimenPath} does not exist.`);
				}
			}
		});
		context.subscriptions.push(editTestCaseCommand);

		const forkTestCaseCommand = vscode.commands.registerCommand('extension.shatterForkAutoTestcase', async (node: CommonDisplayNode) => {
			const specimenId = node.key;
			if (!specimenId) {
				return;
			}

			const { specimental, functionState } = getActiveStates(extensionState);
			if (!specimental || !functionState) {
				//TODO: error
				return;
			}
			//	ask for a name
			//	copy to that name
			const newTestCaseName = await vscode.window.showInputBox({
				prompt: 'Enter a name for the test case',
				placeHolder: 'Custom test case name',
				//	TODO: make sure it's a valid filename; limit the possible values?
				validateInput: (value) => value !== undefined && value.trim().length > 0 ? undefined : 'Please enter a name for the test case',
			});

			if (!newTestCaseName) {
				//TODO: error
				return;
			}
			const newId: SpecimenId = `custom-${newTestCaseName}`;
			if (newId in functionState.specimens) {
				//TODO: error
				return;
			}

			//	if persistable and the base test is already persisted
			if (specimenBaseDirectory && specimental.fileUnderTest) {
				// function forkTest(storageBaseDirectory: AbsolutePath, specimental: Specimental, sourceFileUnderTestPath: RelativePath, testCaseName: SpecimenId) {

				const newSpecimental = forkTest(specimenBaseDirectory, specimental, newId, newTestCaseName);
				functionState.specimens[newId] = {
					...specimental,
					clusterKey: specimental.clusterKey,
					fileUnderTest: specimental.fileUnderTest,
					specimen: specimental.specimen,
				};
				if (vscode.window.activeTextEditor?.document.languageId === 'typescript') {
					refresh(vscode.window.activeTextEditor, extensionState, providers);
					if (newSpecimental.specimenPath && fs.existsSync(newSpecimental.specimenPath)) {
						vscode.workspace.openTextDocument(newSpecimental.specimenPath).then((doc) => {
							vscode.window.showTextDocument(doc, vscode.ViewColumn.One);
						});
					} else {
						vscode.window.showErrorMessage(`Test case ${newSpecimental.specimenPath} does not exist.`);
					}
				}
			}
		});
		context.subscriptions.push(forkTestCaseCommand);
		
		const runTestCaseCommand = vscode.commands.registerCommand('extension.shatterRunTestcase', async (node: CommonDisplayNode) => {
		});
		context.subscriptions.push(runTestCaseCommand);
		
		const runTestCasesCommand = vscode.commands.registerCommand('extension.shatterRunTestcases', async (node: CommonDisplayNode) => {
		});
		context.subscriptions.push(runTestCasesCommand);

		vscode.window.onDidChangeActiveTextEditor(editor => {
			if (editor?.document.fileName) {
				updateSelectedFile();
			}
		}, null, context.subscriptions);

		//	overkill to refresh on every change?  TODO: see if there's a performance hit; at least we want to regenerate the function list
		vscode.workspace.onDidChangeTextDocument(event => {
			const editor = vscode.window.activeTextEditor;
			if (editor?.document.fileName) {
				updateSelectedFile();
			}
		}, null, context.subscriptions);

		//	TODO
		vscode.workspace.onDidOpenTextDocument(document => {
			const editor = vscode.window.activeTextEditor;
			if (editor?.document.fileName) {
				updateSelectedFile();
			}
		}, null, context.subscriptions);
		//	TODO: what to do when a document is closed?

		//	TODO: fix the ugly hard-coding of 'src'; that can't be right for a standalone extension
		//	TODO: just make people import shatterproof module in their projects; don't try to be magical about it
		//	shatterproof needs an existence outside VSCode anyway
		const extensionSource = join(context.extensionPath, 'src');
		const autotestFromEditorContextMenu = await vscode.commands.registerCommand('extension.shatterAutotestFromEditorContextMenu', async () => {
			if (vscode.window.activeTextEditor?.document.languageId === 'typescript') {
				const selection = vscode.window.activeTextEditor.selection;
				const cursorPosition = selection.active;
				const document = vscode.window.activeTextEditor.document;

				const functionMeta = getFunctionNodeAtCursor(cursorPosition, document);
				if (functionMeta) {
					const functionName = functionMeta.name;
					if (!functionName) {
						throw new Error(`Top level anonymous functions are not supported`);
					}

					const absoluteFileUnderTest = document.fileName as AbsolutePath;
					await autotestFunction(absoluteFileUnderTest, functionName);
				} else {
					vscode.window.showErrorMessage('Select a function or place the cursor inside a function.');
				}
			}
		});
		context.subscriptions.push(autotestFromEditorContextMenu);

		const autotestFromFunctionViewContainerMenu = vscode.commands.registerCommand('extension.shatterAutotestFromFunctionViewContainer', (item) => {
			const filename = vscode.window.activeTextEditor?.document.fileName ?? extensionState.activeFile;
			if (!filename) {
				//	TODO: is this a reasonable situation?
				return;
			}

			autotestFunction(filename as AbsolutePath, item.key);
		});
		context.subscriptions.push(autotestFromFunctionViewContainerMenu);

		// vscode.languages.registerCodeActionsProvider(
		// 	{ scheme: 'file', language: 'typescript' },
		// 	{
		// 		provideCodeActions: (document, range) => {
		// 			console.log(`provideCodeActions called`);
		// 			return [
		// 				{
		// 					command: 'extension.shatterAutotestContextFromEditor',
		// 					title: 'Shatter Autotest',
		// 					tooltip: 'Generate autotest for selected function',
		// 				},
		// 			];
		// 		},
		// 	}
		// );

		const retestCommand = await vscode.commands.registerCommand('extension.shatterRetestFromEditorContextMenu', async () => {
			console.log(`there was an attempt`);
		});
		context.subscriptions.push(retestCommand);

		const retestContextMenu = vscode.commands.registerCommand('extension.shatterRetestFromFunctionViewContainer', () => {
			console.log(`there was an attempt`);
		});
		context.subscriptions.push(retestContextMenu);

		const shatterAddTestcase = vscode.commands.registerCommand('extension.shatterAddTestcase', () => {
			console.log(`there was an attempt`);
		});
		context.subscriptions.push(shatterAddTestcase);

		// vscode.languages.registerCodeActionsProvider(
		// 	{ scheme: 'file', language: 'typescript' },
		// 	{
		// 		provideCodeActions: (document, range) => {
		// 			console.log(`provideCodeActions called`);
		// 			return [
		// 				{
		// 					command: 'extension.shatterRetestContext',
		// 					title: 'Shatter Retest',
		// 					tooltip: 'Retest selected function',
		// 				},
		// 			];
		// 		},
		// 	}
		// );


		if (vscode.window.activeTextEditor?.document.languageId === 'typescript') {
			updateSelectedFile();
		}

		//	TODO: some sort of status display during execution
		//	TODO: show the sidebar when running
		async function autotestFunction(absoluteSourceFilename: AbsolutePath, functionName: string) {
			const allTsConfigs: string[] = [];
			const allPackageJsons: string[] = [];
			const allNodeModules: string[] = [];
			const allWorkspaceFolders: string[] = [];

			const editor = vscode.window.activeTextEditor;
			if (editor?.document.languageId === 'typescript') {
				workspaceRoots?.forEach((absoluteFolderPath) => {
					//	TODO: do we know whether the path is already absolute always?
					const found = findFilesInHierarchy(editor.document.fileName, absoluteFolderPath, {
						tsconfig: (absoluteFilename, stat) => absoluteFilename.endsWith('tsconfig.json') && stat.isFile(),
						packageJson: (absoluteFilename, stat) => absoluteFilename.endsWith('package.json') && stat.isFile(),
						nodeModules: (absoluteFilename, stat) => absoluteFilename.endsWith('node_modules') && stat.isDirectory(),
					});

					allTsConfigs.push(...(found.tsconfig || []));
					allPackageJsons.push(...(found.packageJson || []));
					allNodeModules.push(...(found.nodeModules || []));
					allWorkspaceFolders.push(absoluteFolderPath);
				});
			}

			const modulePaths = [...allWorkspaceFolders, ...allNodeModules];

			console.log(`BEGIN THE AUTOTEST of ${functionName} in ${absoluteSourceFilename}`);

			extensionState.activeCoverage = undefined;
			extensionState.activeSpecimenId = undefined;
			for (const provider of Object.values(providers)) {
				provider.refresh([]);
			}

			vscode.commands.executeCommand("shatter-execution-paths.focus");
			try {
				extensionState.runningAutotestFunction = functionName;
				const relativeSourceFilename = (() => {
					const inWorkspaceRelativePath = asRelativePath(absoluteSourceFilename);
					if (inWorkspaceRelativePath) {
						return inWorkspaceRelativePath as RelativePath;
					}

					const relativePath = path.relative(process.cwd(), absoluteSourceFilename);
					return relativePath as RelativePath;
				})();

				await shatterAutotest(modulePaths,
					absoluteSourceFilename,
					relativeSourceFilename,
					functionName, (results: AutotestResults) => {
						const { fileState, functionState } = getActiveStates(extensionState);
						if (!fileState || !functionState) {
							return;
						}

						// console.log(`refreshing function node to display = ${functionName} in ${filename}`);
						// console.log(`keys ${JSON.stringify(Array.from(Object.keys(filestate.functionStates) ?? []))} => ${JSON.stringify(functionState)}`);
						// console.log(`new functionStates entries ${JSON.stringify(filestate.functionStates)}`);
						// console.log(`>>>>>>>>>>>>>>>>>>>  ${JSON.stringify(extensionState.fileStates[filename].functionStates)}`);
						// console.log(`===================  ${JSON.stringify(extensionState.fileStates[filename].functionStates[functionName])}`);
						doSelectFunctionCommand({
							key: functionName,
							label: ''
						});

						results.clusters.forEach((cluster) => {
							cluster.specimens.forEach((specimen) => {
								const existing = functionState.specimens[specimen.id];
								functionState.specimens[specimen.id] = {
									...existing,
									clusterKey: cluster.key,
									specimen,
								};
							});
						});
					}, { shatterproofModuleOverride: extensionSource });
				console.log("END THE AUTOTEST");
				context.workspaceState.update(autotestStorageStateKey, extensionState);
				refresh(editor, extensionState, providers);
			} finally {
				extensionState.runningAutotestFunction = undefined;
			}
		}
	} catch (e: any) {
		console.error(`Unable to load extension ${e}: ${e.stack}`);
	}
}

function doSelectFile(editor: vscode.TextEditor | undefined, context: vscode.ExtensionContext, extensionState: ExtensionState, absoluteSourceFilename: AbsolutePath, providers: Providers) {
	if (extensionState.activeFile !== absoluteSourceFilename) {
		extensionState.activeFile = absoluteSourceFilename;
		extensionState.activeFunction = undefined;
		extensionState.activeCoverage = undefined;
		extensionState.activeSpecimenId = undefined;
	}

	const functions = findFunctions(absoluteSourceFilename);
	/*
	Typescript didn't like this spread
		extensionState.fileStates[filename] = {
			functionStates: {},
			...extensionState.fileStates[filename],
			functions,
		};

	 */
	if (extensionState.fileStates[absoluteSourceFilename]) {
		extensionState.fileStates[absoluteSourceFilename].functions = functions;
	} else {
		extensionState.fileStates[absoluteSourceFilename] = {
			functionStates: {},
			functions,
		};
	}

	refresh(editor, extensionState, providers);
}

//	TODO: consolidate with findFunctions in transform.ts
function getFunctionNodeAtCursor(cursorPosition: vscode.Position, document: vscode.TextDocument): FunctionMeta | undefined {
	const sourceCode = document.getText();
	const sourceFile = ts.createSourceFile(document.fileName, sourceCode, ts.ScriptTarget.Latest, true);

	function findFunction(node: ts.Node): ts.FunctionDeclaration | ts.MethodDeclaration | undefined {
		if (ts.isFunctionDeclaration(node) || ts.isMethodDeclaration(node)) {
			const functionStartPos = node.pos;
			const functionStartLC = ts.getLineAndCharacterOfPosition(sourceFile, node.pos);
			const functionEndPos = node.body?.end ?? node.end;
			const functionEndLC = ts.getLineAndCharacterOfPosition(sourceFile, functionEndPos);

			if (functionStartPos !== undefined && functionEndPos !== undefined) {
				const functionRange = new vscode.Range(
					document.positionAt(functionStartPos),
					document.positionAt(functionEndPos)
				);
				if (functionRange.contains(cursorPosition)) {
					return node;
				}
			}
		}
		return ts.forEachChild(node, findFunction);
	}

	const f = findFunction(sourceFile);
	if (!f) {
		return undefined;
	}

	const name = (f as ts.FunctionDeclaration).name?.text;
	if (name) {
		return {
			name,
			startLine: f.getStart(),
			endLine: f.getEnd(),
		};
	}
}

export function deactivate() { }

// Define a custom TreeDataProvider for the result clusters
class CommonTreeDataProvider implements vscode.TreeDataProvider<CommonDisplayNode> {
	private _onDidChangeTreeData: vscode.EventEmitter<CommonDisplayNode | undefined | void> = new vscode.EventEmitter<CommonDisplayNode | undefined>();
	readonly onDidChangeTreeData: vscode.Event<CommonDisplayNode | undefined | void> = this._onDidChangeTreeData.event;

	private roots: CommonDisplayNode[] | undefined;

	// Initialize empty
	constructor(private options?: {
		command?: Pick<vscode.Command, 'command' | 'title'>,
		stateIcons?: Record<string, Record<'dark' | 'light', string>>
	}) {
		this.roots = undefined;
	}

	// update notify the tree view.
	//	TODO: if the tree provider is going to know about AutotestResults
	//	then it should do the conversion also
	refresh(roots: CommonDisplayNode[] | undefined) {
		this.roots = roots;

		// console.log(`firing onchange with ${JSON.stringify(roots)}}`);

		this._onDidChangeTreeData.fire();
	}

	// Get the children of a tree node.
	getChildren(element?: CommonDisplayNode): Thenable<CommonDisplayNode[]> {
		if (!element) {
			// Return the root nodes if element is undefined as that indicates the beginning of traversal
			return Promise.resolve(this.roots ? this.roots : []);
		}
		const children = element.children || [];
		return Promise.resolve(children);
	}

	// Get the parent of a tree node.
	getParent(element: CommonDisplayNode): CommonDisplayNode | null {
		return null; // We're not using parent-child relationships.
	}

	// Get the tree item for a node.
	getTreeItem(element: CommonDisplayNode): vscode.TreeItem {
		const treeItem = new vscode.TreeItem(element.label);
		treeItem.collapsibleState = element.children ? vscode.TreeItemCollapsibleState.Expanded : vscode.TreeItemCollapsibleState.None;
		treeItem.collapsibleState = element.children ? vscode.TreeItemCollapsibleState.Collapsed : vscode.TreeItemCollapsibleState.None;
		//	TODO: tooltip should be expanded (but still bounded) parameter list
		treeItem.tooltip = element.label;
		if (this.options?.command) {
			treeItem.command = {
				...this?.options.command,
				arguments: [element],
			};
		}
		if (this.options?.stateIcons && element.state) {
			treeItem.iconPath = this.options.stateIcons[element.state];
		}
		treeItem.contextValue = element.contextValue;
		return treeItem;
	}
}

function findFilesInHierarchy<K extends string>(
	absoluteFilename: string,
	absoluteRootDirectory: string,
	matchers: Record<K, (filename: string, stat: fs.Stats) => boolean>,
): Partial<Record<K, string[]>> {
	const foundFiles: Partial<Record<K, string[]>> = {};

	let absoluteCurrentDir = path.dirname(absoluteFilename);
	while (absoluteCurrentDir !== absoluteRootDirectory) {
		fs.readdirSync(absoluteCurrentDir).forEach((file) => {
			const absoluteFullPath = path.join(absoluteCurrentDir, file) as AbsolutePath;
			const stat = fs.statSync(absoluteFullPath);
			for (const key of Object.keys(matchers)) {
				const k: keyof typeof foundFiles = key as any;
				const matcher = matchers[k];

				const matches = matcher(absoluteFullPath, stat);
				if (matches) {
					if (!(key in foundFiles)) {
						foundFiles[k] = [];
					}

					const workspaceRelativePath = asRelativePath(absoluteFullPath);
					if (workspaceRelativePath) {
						foundFiles[k]?.push(workspaceRelativePath);
					}
				}
			}
		});

		const parentDir = path.dirname(absoluteCurrentDir);
		if (parentDir === absoluteCurrentDir) {
			break;
		}

		absoluteCurrentDir = parentDir;
	}

	return foundFiles;
}