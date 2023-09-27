import * as fs from 'fs'; //TODO: use VSCode fs
import * as path from 'path';
import { join } from 'path';
import * as ts from 'typescript';
import * as vscode from 'vscode';
import { AutotestResults, ResultCluster, shatterAutotest } from '../core/shatter';
import { Outcome, RunResult } from '../core/supervisor';
import { FunctionMeta, findFunctions } from '../core/transform';
import { Specimen } from '../core/generator';

interface CommonDisplayNode {
	label: string;
	children?: CommonDisplayNode[];
	key?: string,
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
};

type FileState = {
	functions: FunctionMeta[];
	functionStates: Record<string, FunctionState>;
};

type CoverageSelection = 'all'
	| 'missed'
	| { clusterKeys: string[] };

type ExtensionState = {
	runningAutotestFunction?: string;
	fileStates: Record<string, FileState>
	activeFile?: string;
	activeFunction?: string;
	activeCoverage?: CoverageSelection;
	activeTestCase?: string;
};

interface Providers {
	functionsListProvider: CommonTreeDataProvider,
	clustersListProvider: CommonTreeDataProvider,
	runResultProvider: CommonTreeDataProvider,
	testCaseProvider: CommonTreeDataProvider,
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

const refresh = (editor: vscode.TextEditor | undefined, extensionState: ExtensionState, providers: Providers) => {
	const { functionsListProvider, clustersListProvider, runResultProvider, testCaseProvider } = providers;

	const filename = extensionState.activeFile;
	if (!filename) {
		//	TODO: clear functions list, clusters list, branches list, test cases list
		return;
	}

	const fileState = extensionState.fileStates[filename];
	if (!fileState || !fileState.functions) {
		//	TODO: clear what needs clearing
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

	if (!extensionState.activeFunction) {
		return;
	}

	const func = fileState.functions.find((f) => f.name === extensionState.activeFunction);
	if (!func) {
		return;
	}

	const functionState = fileState.functionStates[extensionState.activeFunction];
	if (!functionState) {
		// console.log(`nonono results for filename "${filename}" and function "${extensionState.activeFunction}" - ${JSON.stringify(fileState.functionStates)}`)
		return;
	};

	const results = functionState?.autotest;
	if (!results) {
		// console.log(`function state keys ${JSON.stringify(Object.keys(fileState.functionStates))}`)
		// console.log(`function states ${JSON.stringify(fileState.functionStates)}`)
		// console.log(`file states ${JSON.stringify(extensionState.fileStates)}`)
		return;
	}

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
	for (let line = func.startLine; line <= func.endLine; line++) {
		if (functionState.autotest.instrumentedLines.has(line)) {
			functionInstrumentedLines.add(line);
		}
	}

	const formatter = Intl.NumberFormat("en-US", { style: "percent" });
	//	TODO: sort by coverage
	results.clusters.forEach((cluster) => {
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

	const coverage = extensionState.activeCoverage;
	const clusters = (() => {
		if (!coverage) {
			return [];
		}
		if (coverage === 'all' || coverage === 'missed') {
			return functionState.autotest.clusters;
		}
		if ('clusterKeys' in coverage) {
			return functionState.autotest.clusters.filter((cluster) => coverage.clusterKeys.includes(cluster.key));
		}
		throw new Error(`unhandled coverage selection ${JSON.stringify(coverage)}`);
	})();

	const mode = coverage === 'missed' ? 'missed' : 'covered';

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
		if (clusters.length > 0) {
			const covered = new Set(clusters.flatMap((cluster) => cluster.lines));
			const lines = (() => {
				if (mode === 'missed') {
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

			const decorationType = mode === 'covered' ? coveredDecorationType : missedDecorationType;

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

	if (extensionState.activeCoverage === 'missed') {
		runResultProvider.refresh([]);
		testCaseProvider.refresh([]);
		return;
	}

	const specimEntries = clusters.flatMap(c =>
		c.specimens.map((specimen): [string, Specimen] => [specimen.id, specimen])
	);
	const runResultNodes: CommonDisplayNode[] = clusters.flatMap(c => c.results.map((result, i) => {
		const parametersNode = {
			label: shortString(result.serializedParameters),
			key: `parameters://${c.key}/${i}`
		};
		return parametersNode;
	}));
	runResultProvider.refresh(runResultNodes);

	if (!extensionState.activeTestCase) {
		return;
	}
	const rr = /(?<which>parameters|result):\/\/(?<clusterKey>[^/]+)\/(?<caseNumber>[0-9]+)/;
	const match = rr.exec(extensionState.activeTestCase);
	if (!match || !match.groups) {
		return;
	}

	const which = match.groups.which;
	const clusterKey = match.groups.clusterKey;
	const caseNumber = parseInt(match.groups.caseNumber);

	const cluster = functionState.autotest.clusters.find((c) => c.key === clusterKey);
	if (!cluster) {
		return;
	}

	const result = cluster.results[caseNumber];
	if (!result) {
		return;
	}

	//	TODO: make this cleaner, ideally like JSON.stringify(...)
	const metadataNode = {
		label: `Duration ${result.duration}ms`
	};
	const resultNode = visit('Result', result.output ?? result.error, 3);

	const specimen = cluster.specimens.find(s => s.id === result.specimenId);
	if (!specimen) {
		console.error(`Unable to find specimen ${result.specimenId}`);
		return;
	}

	const parametersNode = visit('Parameters', specimen.parameters, 3);
	const testCaseNodes: CommonDisplayNode[] = [
		metadataNode,
		parametersNode,
		resultNode,
	];

	testCaseProvider.refresh(testCaseNodes);
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

const doSelectCluster = (editor: vscode.TextEditor, extensionState: ExtensionState, providers: Providers,
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

const doSelectTestCase = (editor: vscode.TextEditor, extensionState: ExtensionState, providers: Providers,
	testCase: string) => {
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

	extensionState.activeTestCase = testCase;
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
		const decoration = { range: line.range, hoverMessage: `Line ${lineNumber}: ${line.text}` };
		decorationsArray.push(decoration);
		lines.push(lineNumber);
	};
	console.log(`highlightLinesInEditor ${JSON.stringify(lines)}}`);

	editor.setDecorations(decorationType, decorationsArray);
}

export function activate(context: vscode.ExtensionContext) {
	//	TODO: if there's an open editor when the extension is activated, select that file
	const extensionState: ExtensionState = {
		fileStates: {},
	};

	//	TODO: Refresh functions list view contents on change of editor
	const functionsListProvider = new CommonTreeDataProvider({
		command: 'extension.shatterSelectFunction',
		title: 'Functions',
	});
	context.subscriptions.push(
		vscode.window.registerTreeDataProvider("shatter-functions-list", functionsListProvider));

	const clustersListProvider = new CommonTreeDataProvider({
		command: 'extension.shatterSelectCluster',
		title: 'Execution Paths',
	});
	context.subscriptions.push(
		vscode.window.registerTreeDataProvider("shatter-execution-paths", clustersListProvider));

	const runResultProvider = new CommonTreeDataProvider({
		command: 'extension.shatterSelectTestCase',
		title: 'Test Case Detail',
	});
	context.subscriptions.push(
		vscode.window.registerTreeDataProvider("shatter-run-results", runResultProvider));

	const testCaseProvider = new CommonTreeDataProvider();
	context.subscriptions.push(
		vscode.window.registerTreeDataProvider("shatter-test-case", testCaseProvider));

	const providers = {
		functionsListProvider,
		clustersListProvider,
		runResultProvider,
		testCaseProvider,
	};

	const updateSelectedFile = () => {
		//	_filename and filename should be the same
		const filename = vscode.window.activeTextEditor?.document.fileName;
		if (!filename) {
			//	TODO: clear functions list
			return;
		}
		doSelectFile(vscode.window.activeTextEditor, extensionState, filename, providers);
	};

	//	call after switching files, changing contents of the editor, or running tests
	const doSelectFunctionCommand = (node: CommonDisplayNode) => {
		if (vscode.window.activeTextEditor) {
			const functionName: string = node.key || "";
			doSelectFunction(vscode.window.activeTextEditor, extensionState, providers, functionName);
		}
	};

	const doSelectClusterCommand = (node: CommonDisplayNode) => {
		if (vscode.window.activeTextEditor) {
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
				doSelectCluster(vscode.window.activeTextEditor, extensionState, providers, selection);
			}
		}
	};

	const doSelectTestCaseCommand = (node: CommonDisplayNode) => {
		if (vscode.window.activeTextEditor) {
			const testCase: string = node.key || "";
			doSelectTestCase(vscode.window.activeTextEditor, extensionState, providers, testCase);
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

	context.subscriptions.push(vscode.window.onDidChangeActiveTextEditor(editor => {
		if (editor?.document.fileName) {
			updateSelectedFile();
		}
	}, null, context.subscriptions));

	//	overkill to refresh on every change?  TODO: see if there's a performance hit; at least we want to regenerate the function list
	context.subscriptions.push(vscode.workspace.onDidChangeTextDocument(event => {
		const editor = vscode.window.activeTextEditor;
		if (editor?.document.fileName) {
			updateSelectedFile();
		}
	}, null, context.subscriptions));

	//	TODO
	vscode.workspace.onDidOpenTextDocument(document => { });
	//	TODO: what to do when a document is closed?

	//	TODO: fix the ugly hard-coding of 'src'; that can't be right for a standalone extension
	//	TODO: just make people import shatterproof module in their projects; don't try to be magical about it
	//	shatterproof needs an existence outside VSCode anyway
	const extensionSource = join(context.extensionPath, 'src');

	const autotestCommand = vscode.commands.registerCommand('extension.shatterAutotest', async () => {
		const editor = vscode.window.activeTextEditor;
		ts.ScriptSnapshot.fromString('');
		//	TODOTODO: initialize empty results sidebar

		if (editor && editor.document.languageId === 'typescript') {
			const selection = editor.selection;
			const cursorPosition = selection.active;
			const document = editor.document;

			if (isCursorInFunctionName(cursorPosition, document, editor)) {
				const functionMeta = getFunctionNodeAtCursor(cursorPosition, document);

				if (functionMeta) {
					const functionName = functionMeta.name;
					if (!functionName) {
						throw new Error(`Top level anonymous functions are not supported`);
					}
					await autotestFunction(document.fileName, functionName);
				} else {
					console.log(`function node not found`);
				}
			} else {
				vscode.window.showErrorMessage('Select a function or place the cursor inside a function.');
			}
		}
	});

	context.subscriptions.push(autotestCommand);

	const autotestEditorContextMenu = vscode.commands.registerCommand('extension.shatterAutotestContext', () => {
		vscode.commands.executeCommand('extension.shatterAutotest');
	});
	context.subscriptions.push(autotestEditorContextMenu);

	const autotestFunctionViewContainerMenu = vscode.commands.registerCommand('extension.shatterAutotestFunctionViewContainer', (item) => {
		const filename = vscode.window.activeTextEditor?.document.fileName;
		if (!filename) {
			//	TODO: is this a reasonable situation?
			return;
		}
		autotestFunction(filename, item.key);
	});
	context.subscriptions.push(autotestFunctionViewContainerMenu);

	const retestFunctionViewContainerMenu = vscode.commands.registerCommand('extension.shatterRetestFunctionViewContainer', (item) => {
		// console.log(`retestFunctionViewContainerMenu called with ${JSON.stringify(item)}`);
	});
	context.subscriptions.push(retestFunctionViewContainerMenu);

	vscode.languages.registerCodeActionsProvider(
		{ scheme: 'file', language: 'typescript' },
		{
			provideCodeActions: (document, range) => {
				console.log(`provideCodeActions called`);
				return [
					{
						command: 'extension.shatterAutotestContext',
						title: 'Shatter Autotest',
						tooltip: 'Generate autotest for selected function',
					},
				];
			},
		}
	);

	const retestCommand = vscode.commands.registerCommand('extension.shatterRetest', async () => {
		console.log(`there was an attempt`);
	});

	context.subscriptions.push(retestCommand);

	const retestContextMenu = vscode.commands.registerCommand('extension.shatterRetestContext', () => {
		vscode.commands.executeCommand('extension.shatterRetest');
	});

	vscode.languages.registerCodeActionsProvider(
		{ scheme: 'file', language: 'typescript' },
		{
			provideCodeActions: (document, range) => {
				console.log(`provideCodeActions called`);
				return [
					{
						command: 'extension.shatterRetestContext',
						title: 'Shatter Retest',
						tooltip: 'Retest selected function',
					},
				];
			},
		}
	);

	context.subscriptions.push(retestContextMenu);

	if (vscode.window.activeTextEditor) {
		updateSelectedFile();
	}

	//	TODO: some sort of status display during execution
	//	TODO: show the sidebar when running
	async function autotestFunction(filename: string, functionName: string) {
		const allTsConfigs: string[] = [];
		const allPackageJsons: string[] = [];
		const allNodeModules: string[] = [];
		const allWorkspaceFolders: string[] = [];

		const editor = vscode.window.activeTextEditor;
		if (editor) {
			vscode.workspace.workspaceFolders?.forEach((folder) => {
				const found = findFilesInHierarchy(editor.document.fileName, vscode.workspace.rootPath || '', {
					tsconfig: (filename, stat) => filename.endsWith('tsconfig.json') && stat.isFile(),
					packageJson: (filename, stat) => filename.endsWith('package.json') && stat.isFile(),
					nodeModules: (filename, stat) => filename.endsWith('node_modules') && stat.isDirectory(),
				});

				allTsConfigs.push(...(found.tsconfig || []));
				allPackageJsons.push(...(found.packageJson || []));
				allNodeModules.push(...(found.nodeModules || []));
				allWorkspaceFolders.push(folder.uri.fsPath);
			});
		}

		const modulePaths = [...allWorkspaceFolders, ...allNodeModules];

		console.log(`BEGIN THE AUTOTEST of ${functionName} in ${filename}`);
		vscode.commands.executeCommand("shatter-execution-paths.focus");
		try {
			extensionState.runningAutotestFunction = functionName;

			await shatterAutotest(modulePaths,
				filename,
				context.storageUri?.fsPath,
				functionName, (results: AutotestResults) => {
					extensionState.activeFile = filename;
					let filestate: FileState | undefined = extensionState.fileStates[filename];
					if (!filestate) {
						const functions = findFunctions(filename);
						filestate = {
							functions,
							functionStates: {},
						};
						extensionState.fileStates[filename] = filestate;
					}
					const functionState: FunctionState = {
						autotest: results,
					};
					filestate.functionStates[functionName] = functionState;

					// console.log(`refreshing function node to display = ${functionName} in ${filename}`);
					// console.log(`keys ${JSON.stringify(Array.from(Object.keys(filestate.functionStates) ?? []))} => ${JSON.stringify(functionState)}`);
					// console.log(`new functionStates entries ${JSON.stringify(filestate.functionStates)}`);
					// console.log(`>>>>>>>>>>>>>>>>>>>  ${JSON.stringify(extensionState.fileStates[filename].functionStates)}`);
					// console.log(`===================  ${JSON.stringify(extensionState.fileStates[filename].functionStates[functionName])}`);
					doSelectFunctionCommand({
						key: functionName,
						label: ''
					});
				}, { shatterproofModuleOverride: extensionSource });
			console.log("END THE AUTOTEST");
			refresh(editor, extensionState, providers);
		} finally {
			extensionState.runningAutotestFunction = undefined;
		}
	}
}

function doSelectFile(editor: vscode.TextEditor | undefined, extensionState: ExtensionState, filename: string, providers: Providers) {
	extensionState.activeFile = filename;

	const functions = findFunctions(filename);
	/*
	Typescript didn't like this spread
		extensionState.fileStates[filename] = {
			functionStates: {},
			...extensionState.fileStates[filename],
			functions,
		};

	 */
	if (extensionState.fileStates[filename]) {
		extensionState.fileStates[filename].functions = functions;
	} else {
		extensionState.fileStates[filename] = {
			functionStates: {},
			functions,
		};
	}

	refresh(editor, extensionState, providers);
}

function isCursorInFunctionName(
	cursorPosition: vscode.Position,
	document: vscode.TextDocument,
	editor: vscode.TextEditor
): boolean {
	const line = document.lineAt(cursorPosition.line).text;
	return line.includes('function');
}

//	TODO: consolidate with findFunctions in transform.ts
function getFunctionNodeAtCursor(cursorPosition: vscode.Position, document: vscode.TextDocument): FunctionMeta | undefined {
	const sourceCode = document.getText();
	const sourceFile = ts.createSourceFile(document.fileName, sourceCode, ts.ScriptTarget.Latest, true);

	function findFunction(node: ts.Node): ts.Node | undefined {
		if (node.kind === ts.SyntaxKind.FunctionDeclaration || node.kind === ts.SyntaxKind.MethodDeclaration) {
			const functionNode = node as ts.FunctionDeclaration | ts.MethodDeclaration;
			const functionStartPos = functionNode.name?.getStart(sourceFile);
			const functionEndPos = functionNode.getEnd();

			if (functionStartPos !== undefined && functionEndPos !== undefined) {
				const functionRange = new vscode.Range(
					document.positionAt(functionStartPos),
					document.positionAt(functionEndPos)
				);
				if (functionRange.contains(cursorPosition)) {
					return functionNode;
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
			node: f as ts.FunctionDeclaration,
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
	constructor(private command?: Pick<vscode.Command, 'command' | 'title'>) {
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
		if (this.command) {
			treeItem.command = {
				...this.command,
				arguments: [element],
			};
		}
		return treeItem;
	}
}

function findFilesInHierarchy<K extends string>(
	filename: string,
	rootDirectory: string,
	matchers: Record<K, (filename: string, stat: fs.Stats) => boolean>,
): Partial<Record<K, string[]>> {
	const foundFiles: Partial<Record<K, string[]>> = {};

	let currentDir = path.dirname(filename);
	while (currentDir !== rootDirectory) {
		fs.readdirSync(currentDir).forEach((file) => {
			const fullPath = path.join(currentDir, file);
			const stat = fs.statSync(fullPath);
			for (const key of Object.keys(matchers)) {
				const k: keyof typeof foundFiles = key as any;
				const matcher = matchers[k];

				const matches = matcher(fullPath, stat);
				if (matches) {
					if (!(key in foundFiles)) {
						foundFiles[k] = [];
					}
					foundFiles[k]?.push(fullPath);
				}
			}
		});

		const parentDir = path.dirname(currentDir);
		if (parentDir === currentDir) {
			break;
		}

		currentDir = parentDir;
	}

	return foundFiles;
}