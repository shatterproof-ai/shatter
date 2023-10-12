import * as fs from 'fs'; //TODO: use VSCode fs
import * as path from 'path';
import * as ts from 'typescript';
import * as vscode from 'vscode';
import { AbsolutePath, RelativePath, Specimen, SpecimenId, isRelativePath, isSpecimenId, joinAbsolute } from '../core/common';
import { isOutcome } from '../core/supervisor';
import { FunctionMeta } from '../core/transform';
import { CoverageSelection, ExtensionState, Specimental, cleanUpExtensionState, isCoverageSelection, onPersistedSpecimenLoad } from './common';
import { CommonDisplayNode, DisplayProvider, DisplayProviders, Highlighter, SelectedElements, doSelectCluster, doSelectFile, doSelectFunction, doSelectTestCase, filterClustersForCoverage, findClustersForCoverage, findFunction, findNode, findSpecimen, refresh } from './display';
import { forkSpecimen, loadExpected, loadPersistedSpecimen, loadPersistedSpecimens, saveSpecimen } from './persistence';
import { TestLifecycle, autotestFunction, retestFunction } from './run';

const COMMANDS = {
	shatterAddTestcase: 'extension.shatterAddTestcase',
	shatterAutotestFromEditorContextMenu: 'extension.shatterAutotestFromEditorContextMenu',
	shatterAutotestFromFunctionViewContainer: 'extension.shatterAutotestFromFunctionViewContainer',
	shatterEditCustomTestcase: 'extension.shatterEditCustomTestcase',
	shatterForkAutoTestcase: 'extension.shatterForkAutoTestcase',
	shatterMakeTestcaseNonPersistent: 'extension.shatterMakeTestcaseNonPersistent',
	shatterMakeTestcasePersistent: 'extension.shatterMakeTestcasePersistent',
	shatterRetestFromEditorContextMenu: 'extension.shatterRetestFromEditorContextMenu',
	shatterRetestFromFunctionViewContainer: 'extension.shatterRetestFromFunctionViewContainer',
	shatterRunClustersTestcases: 'extension.shatterRunClustersTestcases',
	shatterRunFunctionTestcases: 'extension.shatterRunFunctionTestcases',
	shatterRunTestcase: 'extension.shatterRunTestcase',
	shatterSelectCluster: 'extension.shatterSelectCluster',
	shatterSelectFunction: 'extension.shatterSelectFunction',
	shatterSelectTestCase: 'extension.shatterSelectTestCase',
	shatterResetLocalFromFunctionViewContainer: 'extension.shatterResetLocalFromFunctionViewContainer',
};

const autotestStorageStateKey = "autotestState_7";

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

//	the stuff that's selected in the UI is not necessarily what's active
function getSelectedElements(providers: DisplayProviders, extensionState: ExtensionState) {
	const selected: SelectedElements = {};
	selected.selectedFile = getSelectedFile(providers, extensionState);
	if (selected.selectedFile) {
		selected.selectedFunction = getSelectedFunction(providers, extensionState, selected.selectedFile);
		if (selected.selectedFunction) {
			selected.coverage = getSelectedCoverage(providers, extensionState, selected.selectedFunction);
			selected.specimental = getSelectedSpecimenId(providers, extensionState, selected.selectedFunction);
		}
	}

	return selected;
}

function getSelectedFile(providers: DisplayProviders,
	extensionState: ExtensionState): SelectedElements['selectedFile'] {
	const filename = vscode.window.activeTextEditor?.document.fileName as AbsolutePath | undefined;
	if (!filename) {
		return undefined;
	}

	const state = extensionState.fileStates[filename];
	if (!state) {
		//  TODO: error
		return undefined;
	}
	return {
		filename,
		state,
	};
}

function getSelectedFunction(providers: DisplayProviders,
	extensionState: ExtensionState, selectedFile: SelectedElements['selectedFile']): SelectedElements['selectedFunction'] {
	const selected = providers.functionsListProvider.getSelected();
	if (!selected || selected.length === 0) {
		return undefined;
	}

	if (selected.length > 1) {
		console.error(`Unexpected multiple selected functions: ${JSON.stringify(selected.map(s => [s.key, s.label]))}`);
	}

	const name = selected[0].key;
	if (typeof name !== 'string') {
		return undefined;
	}

	const state = selectedFile?.state.functionStates[name];
	if (!state) {
		console.error(`Unexpectedly missing function state for ${name} in ${selectedFile?.filename}`);
		return undefined;
	}

	return {
		name,
		state,
	};
}

function getSelectedCoverage(providers: DisplayProviders,
	extensionState: ExtensionState, selectedFunction: SelectedElements['selectedFunction']): SelectedElements['coverage'] {
	const selected = providers.clustersListProvider.getSelected();
	if (!selected || selected.length === 0) {
		return undefined;
	}

	if (selected.length > 1) {
		console.error(`Unexpected multiple selected clusters: ${JSON.stringify(selected.map(s => [s.key, s.label]))}`);
	}

	const selectedCoverage = selected[0].key;
	if (!isCoverageSelection(selectedCoverage)) {
		//  TODO: error
		return undefined;
	}

	const clusters = filterClustersForCoverage(selectedCoverage, selectedFunction?.state.autotest.clusters);
	return {
		selectedCoverage,
		clusters,
	};
}

function getSelectedSpecimenId(providers: DisplayProviders,
	extensionState: ExtensionState, selectedFunction: SelectedElements['selectedFunction']): Specimental | undefined {
	const selected = providers.testCaseListProvider.getSelected();
	if (!selected || selected.length === 0) {
		return undefined;
	}

	if (selected.length > 1) {
		console.error(`Unexpected multiple selected test cases: ${JSON.stringify(selected.map(s => [s.key, s.label]))}`);
	}

	const key = selected[0].key;
	if (!isSpecimenId(key)) {
		return undefined;
	}

	return selectedFunction?.state.specimens[key];
}

/**
 *
 * @deprecated
 */
function asAbsolutePath(workspaceRoot: AbsolutePath, ...pieces: RelativePath[]): AbsolutePath {
	return joinAbsolute(workspaceRoot, ...pieces);
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
		const hoverMessage = undefined;
		// const hoverMessage = `Line ${lineNumber + 1}: ${line.text}`;
		const decoration = { range: line.range, hoverMessage };
		decorationsArray.push(decoration);
		lines.push(lineNumber);
	};
	console.log(`highlightLinesInEditor ${editor.document.fileName} ${JSON.stringify(lines)}}`);

	editor.setDecorations(decorationType, decorationsArray);
}

//	We never write this; it's populated only by reading, and it doesn't exactly
//	match the format on disk
interface ProjectConfiguration {
	baseDirectory?: RelativePath;	//	RELATIVE to the project root
	resultsReferenceDirectory?: RelativePath;	//	RELATIVE to the project root
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
								const pc: ProjectConfiguration = JSON.parse(contents);
								return pc;
							} catch (e) {
								//	TODO: handle error
								const ee = e;
								console.error(`Error parsing ${fileUri}: ${e}`);
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

/*
Operations:
* open test case
* save test case
* add test case

Provide context menu for running a test case from a file

TODO: convert the test case tree view into a test case  manager

How to select test cases?  Per function, per cluster, per test case

*/

function highlighterForEditor(editor: vscode.TextEditor): Highlighter {
	function doHighlighting(decoration: 'covered' | 'missed', linerator: () => Generator<number, void, unknown>) {
		editor?.setDecorations(coveredDecorationType, []);
		editor?.setDecorations(missedDecorationType, []);

		const decorationType = decoration === 'missed' ? missedDecorationType : coveredDecorationType;

		//	TODO: replace with function pointer or pubsub or something that doesn't require passing around the editor object
		highlightLinesInEditor(editor, decorationType, linerator());
	}
	return doHighlighting;

}

const updateSelectedFile = (highlighters: Record<AbsolutePath, Highlighter>, extensionState: ExtensionState, providers: DisplayProviders, selectedElements: SelectedElements) => {
	const filename = vscode.window.activeTextEditor?.document.fileName;
	if (!filename) {
		//	TODO: clear functions list
		return;
	}
	if (vscode.window.activeTextEditor?.document.languageId === 'typescript') {
		doSelectFile(highlighters, extensionState, providers, selectedElements);
	}
};

//	call after switching files, changing contents of the editor, or running tests
const doSelectFunctionCommand = (highlighters: Record<AbsolutePath, Highlighter>, extensionState: ExtensionState, providers: DisplayProviders, node: CommonDisplayNode, selectedElements: SelectedElements) => {
	const editor = vscode.window.activeTextEditor;

	if (editor?.document.languageId === 'typescript') {
		const absoluteFilePath = editor.document.fileName as AbsolutePath;
		let highlighter = highlighters[absoluteFilePath];
		if (!highlighter) {
			highlighter = highlighterForEditor(editor);
			highlighters[absoluteFilePath] = highlighter;
		}

		if (node.contextValue === 'function') {
			//	TODO: check if this is a function name or a test case name
			doSelectFunction(highlighters, extensionState, providers, selectedElements);
		} else if (isSpecimenId(node.key)) {
			doSelectTestCase(highlighters, extensionState, providers, selectedElements);
		}
	}
};

const doSelectClusterCommand = (highlighters: Record<AbsolutePath, Highlighter>, extensionState: ExtensionState, providers: DisplayProviders, node: CommonDisplayNode) => {
	if (vscode.window.activeTextEditor?.document.languageId === 'typescript') {
		const selection: CoverageSelection | undefined = (() => {
			if (node.key) {
				if (node.key.startsWith('cluster://')) {
					const clusterKey = node.key.substring('cluster://'.length);
					return { clusterKey };
				}
				if (node.key === 'covered://') {
					return 'all';
				}
				if (node.key === 'missed://') {
					return 'missed';
				}
				if (isOutcome(node.key)) {
					return node.key;
				}

				throw new Error(`unhandled key ${node.key}`);
			}
		})();
		if (selection) {
			providers.testCaseDetailProvider.refresh([]);
			doSelectCluster(highlighters, extensionState, providers, getSelectedElements(providers, extensionState));
		}
	}
};

const doSelectTestCaseCommand = (highlighters: Record<AbsolutePath, Highlighter>, extensionState: ExtensionState, providers: DisplayProviders, node: CommonDisplayNode) => {
	if (vscode.window.activeTextEditor?.document.languageId === 'typescript') {
		const specimenId: string = node.key || "";
		doSelectTestCase(highlighters, extensionState, providers, getSelectedElements(providers, extensionState));
	}
};

const editTestCase = async (extensionState: ExtensionState, baseDirectory: AbsolutePath | undefined, node: CommonDisplayNode) => {
	const specimenId = node.key;
	if (!baseDirectory || !isSpecimenId(specimenId)) {
		return;
	}

	const specimental = findSpecimen(extensionState, specimenId);
	if (!specimental) {
		return;
	}

	const specimenPath = specimental.specimenPath;
	if (specimenPath && vscode.window.activeTextEditor?.document.languageId === 'typescript') {
		//	vscode FS does not support a nice existence check
		if (fs.existsSync(specimenPath)) {
			vscode.workspace.openTextDocument(specimenPath).then((doc) => {
				vscode.window.showTextDocument(doc, vscode.ViewColumn.One);
			});
		} else {
			vscode.window.showErrorMessage(`Test case ${specimenPath} does not exist.`);
		}
	}
};

function retest(defaultWorkspaceRoot: AbsolutePath, workspaceRoots: AbsolutePath[], context: vscode.ExtensionContext, highlighters: Record<AbsolutePath, Highlighter>, extensionState: ExtensionState, providers: DisplayProviders, node: CommonDisplayNode, specimens: Specimen[], extensionSource: AbsolutePath) {
	const lifeCycler: TestLifecycle = {
		onTestStart(absoluteFilename: AbsolutePath, functionName: string) {
			doSelectFunctionCommand(highlighters, extensionState, providers, {
				key: functionName,
				label: ''
			}, getSelectedElements(providers, extensionState));
		},

		onResult(absoluteFilename, functionName, result) {
			refresh(getSelectedElements(providers, extensionState), extensionState, providers, highlighters);
		},

		onTestEnd(absoluteFilename: AbsolutePath, functionName: string) {
			context.workspaceState.update(autotestStorageStateKey, extensionState);
			extensionState.runningTestFunction = undefined;
			refresh(getSelectedElements(providers, extensionState), extensionState, providers, highlighters);
		},
	};

	const functionNames = new Set<string>(
		specimens.map((specimen) => specimen.functionName)
	);

	if (functionNames.size !== 1) {
		throw new Error(`Unexpectedly ${functionNames.size} functionNames ${specimens.length} specimens under test`);
	}

	const filesUnderTest = new Set<RelativePath>(
		specimens.map((specimen) => specimen.fileUnderTest)
	);

	if (filesUnderTest.size !== 1) {
		throw new Error(`Unexpectedly ${filesUnderTest.size} files from ${specimens.length} specimens under test`);
	}

	const functionName = specimens[0].functionName;
	const relativeSourceFilename = specimens[0].fileUnderTest;
	const absoluteSourceFilename = asAbsolutePath(defaultWorkspaceRoot as AbsolutePath, relativeSourceFilename);

	/*
	retestFunction(extensionState: ExtensionState,
					workspaceRoots: AbsolutePath[],
					absoluteSourceFilename: AbsolutePath,
					relativeSourceFilename: RelativePath,
					providers: DisplayProviders,
					functionName: string,
					specimens:Specimen[],
					lifeCycler: TestLifecycle,
					shatterproofModuleOverride: string) {
	
	*/

	return retestFunction(extensionState, workspaceRoots, absoluteSourceFilename, functionName, specimens, lifeCycler, extensionSource);
}

const forkTestCase = async (extensionState: ExtensionState, baseDirectory: AbsolutePath | undefined, providers: DisplayProviders, highlighters: Record<AbsolutePath, Highlighter>, node: CommonDisplayNode) => {
	const specimenId = node.key;
	if (!isSpecimenId(specimenId)) {
		return;
	}

	const specimental = findSpecimen(extensionState, specimenId);
	if (!specimental) {
		//TODO: error
		return;
	}

	const testCaseNamePattern = /^[a-z0-9_.-]+$/;
	function isValidTestCaseName(s: string | undefined) {
		return s?.match(testCaseNamePattern) !== null;
	}

	//	ask for a name
	//	copy to that name
	const newTestCaseName = await vscode.window.showInputBox({
		prompt: `Enter a name for the test case matching ${testCaseNamePattern}`,
		placeHolder: 'Custom test case name',
		//	TODO: make sure it's a valid filename; limit the possible values?
		validateInput: (value) => isValidTestCaseName(value) ? undefined : 'Please enter a name for the test case',
	});

	if (!newTestCaseName) {
		//TODO: error
		return;
	}
	const newId: SpecimenId = `custom-${newTestCaseName}`;
	const alreadyExisting = findSpecimen(extensionState, newId);
	if (alreadyExisting) {
		//	TODO: error
		return;
	}

	//	if persistable and the base test is already persisted
	if (!baseDirectory) {
		return;
	}
	// function forkTest(storageBaseDirectory: AbsolutePath, specimental: Specimental, sourceFileUnderTestPath: RelativePath, testCaseName: SpecimenId) {
	let newSpecimental: Specimental | undefined = undefined;
	if (specimental.specimenPath) {
		//	forking an already persistent test
		newSpecimental = forkSpecimen(baseDirectory, specimental, newId, newTestCaseName);
	} else {
		//	forking a transient test
		newSpecimental = {
			...specimental,
			specimen: {
				...specimental.specimen,
				id: newId,
				type: 'custom',
				name: newTestCaseName,
			},
			clusterKey: specimental.clusterKey,
			fileUnderTest: specimental.fileUnderTest,
		};
		const specimenFileAbsolutePath = saveSpecimen(baseDirectory, newSpecimental);
		newSpecimental.specimenPath = specimenFileAbsolutePath;
	}

	const nnewSpecimental = newSpecimental;
	Object.entries(extensionState.fileStates).forEach(([absoluteFileName, fileState]) => {
		if (absoluteFileName === nnewSpecimental.fileUnderTest) {
			for (const [functionName, functionState] of Object.entries(fileState.functionStates)) {
				if (functionName === specimental.specimen.functionName) {
					functionState.specimens[newId] = nnewSpecimental;
					return;
				}
			}
		}
	});

	if (vscode.window.activeTextEditor?.document.languageId === 'typescript') {
		refresh(getSelectedElements(providers, extensionState), extensionState, providers, highlighters);
		if (newSpecimental.specimenPath && fs.existsSync(newSpecimental.specimenPath)) {
			vscode.workspace.openTextDocument(newSpecimental.specimenPath).then((doc) => {
				vscode.window.showTextDocument(doc, vscode.ViewColumn.One);
			});
		} else {
			vscode.window.showErrorMessage(`Test case ${newSpecimental.specimenPath} does not exist.`);
		}
	}
};

//	TODO: some sort of status display during execution
//	TODO: show the sidebar when running
async function doAutotest(context: vscode.ExtensionContext, extensionState: ExtensionState, providers: DisplayProviders, highlighters: Record<AbsolutePath, Highlighter>, workspaceRoots: AbsolutePath[], absoluteSourceFilename: AbsolutePath, functionName: string, extensionSource: AbsolutePath) {
	const editor = vscode.window.activeTextEditor;
	if (editor?.document.languageId !== 'typescript') {
		return;
	}

	const lifeCycler: TestLifecycle = {
		onTestStart(absoluteFilename: AbsolutePath, functionName: string) {
			doSelectFunctionCommand(highlighters, extensionState, providers, {
				key: functionName,
				label: ''
			}, getSelectedElements(providers, extensionState));
		},

		onResult(absoluteFilename, functionName, result) {
			refresh(getSelectedElements(providers, extensionState), extensionState, providers, highlighters);
		},

		onTestEnd(absoluteFilename: AbsolutePath, functionName: string) {
			context.workspaceState.update(autotestStorageStateKey, extensionState);
			extensionState.runningTestFunction = undefined;
			refresh(getSelectedElements(providers, extensionState), extensionState, providers, highlighters);
		},
	};

	const relativeSourceFilename = (() => {
		const inWorkspaceRelativePath = asRelativePath(absoluteSourceFilename);
		if (inWorkspaceRelativePath) {
			return inWorkspaceRelativePath as RelativePath;
		}

		const relativePath = path.relative(process.cwd(), absoluteSourceFilename);
		return relativePath as RelativePath;
	})();

	await autotestFunction(extensionState, workspaceRoots, absoluteSourceFilename, relativeSourceFilename, providers, functionName, lifeCycler, extensionSource);
}

export async function activate(context: vscode.ExtensionContext) {
	//	TODO: this all needs to deal in URIs
	const workspaceRoots: AbsolutePath[] = vscode.workspace.workspaceFolders?.map((f) => f.uri.fsPath as AbsolutePath) ?? [];
	const defaultWorkspaceRoot: AbsolutePath | undefined = workspaceRoots[0];

	//	TODO: initialize extensionState in initializeWorkspace
	const extensionState: ExtensionState = cleanUpExtensionState(context.workspaceState.get(autotestStorageStateKey, {}));

	const highlighters: Record<AbsolutePath, Highlighter> = {};
	for (const editor of vscode.window.visibleTextEditors) {
		const absoluteFilename = editor.document.fileName as AbsolutePath;
		highlighters[absoluteFilename] = highlighterForEditor(editor);
	}

	const absolutist = (filename: RelativePath): AbsolutePath => {
		if (!defaultWorkspaceRoot) {
			throw new Error(`Unexpectedly no workspace root for ${filename}`);
		}
		return asAbsolutePath(defaultWorkspaceRoot, filename);
	};

	try {
		const configuration = await initializeWorkspace(defaultWorkspaceRoot, absolutist, extensionState, 'hard');
		const absoluteBaseDirectory = configuration.baseDirectory
			? context.asAbsolutePath(configuration.baseDirectory) as AbsolutePath
			: undefined;

		const providers = initializeTreeViews(context);

		//	TODO: verify that dist works properly
		const extensionSource = context.asAbsolutePath('dist') as AbsolutePath;

		//	needs to be registered as a command because TreeView needs a command to dispatch to
		const selectFunctionCommand = vscode.commands.registerCommand(COMMANDS.shatterSelectFunction, (node) => {
			doSelectFunctionCommand(highlighters, extensionState, providers, node, getSelectedElements(providers, extensionState));
		});
		context.subscriptions.push(selectFunctionCommand);

		//	needs to be registered as a command because TreeView needs a command to dispatch to
		const selectClusterCommand = vscode.commands.registerCommand(COMMANDS.shatterSelectCluster, (node) => doSelectClusterCommand(highlighters, extensionState, providers, node));
		context.subscriptions.push(selectClusterCommand);

		//	needs to be registered as a command because TreeView needs a command to dispatch to
		const selectTestCaseCommand = vscode.commands.registerCommand(COMMANDS.shatterSelectTestCase, (node) => doSelectTestCaseCommand(highlighters, extensionState, providers, node));
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

		const makeTestCasePersistentCommand = vscode.commands.registerCommand(COMMANDS.shatterMakeTestcasePersistent, (node) => makeTestCasePersistent(absoluteBaseDirectory, extensionState, providers, highlighters, node));
		context.subscriptions.push(makeTestCasePersistentCommand);

		const makeTestcaseNotPersistentCommand = vscode.commands.registerCommand(COMMANDS.shatterMakeTestcaseNonPersistent, (node) => makeTestCaseNotPersistent(absoluteBaseDirectory, extensionState, providers, highlighters, node));
		context.subscriptions.push(makeTestcaseNotPersistentCommand);

		const editTestCaseCommand = vscode.commands.registerCommand(COMMANDS.shatterEditCustomTestcase, (node) => editTestCase(extensionState, absoluteBaseDirectory, node));
		context.subscriptions.push(editTestCaseCommand);

		const forkTestCaseCommand = vscode.commands.registerCommand(COMMANDS.shatterForkAutoTestcase, (node) => forkTestCase(extensionState, absoluteBaseDirectory, providers, highlighters, node));
		context.subscriptions.push(forkTestCaseCommand);

		const runTestcaseClustersCommand = vscode.commands.registerCommand(COMMANDS.shatterRunClustersTestcases, async (node: CommonDisplayNode) => {
			if (!isCoverageSelection(node.key)) {
				//	TODO: error
				return;
			}

			const clusters = findClustersForCoverage(extensionState, node.key);
			const specimens = clusters.flatMap(c => c.specimens);
			await retest(defaultWorkspaceRoot, workspaceRoots, context, highlighters, extensionState, providers, node, specimens, extensionSource);
		});
		context.subscriptions.push(runTestcaseClustersCommand);

		const runFunctionTestcasesCommand = vscode.commands.registerCommand(COMMANDS.shatterRunFunctionTestcases, async (node: CommonDisplayNode) => {
			if (!node.key) {
				//	TODO: error
				return;
			}

			const fffff = findFunction(extensionState, node.key);
			if (!fffff) {
				//	TODO: error
				return;
			}

			const [_functionMeta, functionState] = fffff;

			const specimens = Object.values(functionState.specimens).map(s => s.specimen);
			await retest(defaultWorkspaceRoot, workspaceRoots, context, highlighters, extensionState, providers, node, specimens, extensionSource);
		});
		context.subscriptions.push(runFunctionTestcasesCommand);

		const runTestCaseCommand = vscode.commands.registerCommand(COMMANDS.shatterRunTestcase, async (node: CommonDisplayNode) => {
			if (!isSpecimenId(node.key)) {
				return;
			}

			const specimental = findSpecimen(extensionState, node.key);
			if (!specimental) {
				//	TODO: error
				return;
			}

			await retest(defaultWorkspaceRoot, workspaceRoots, context, highlighters, extensionState, providers, node, [specimental.specimen], extensionSource);
		});
		context.subscriptions.push(runTestCaseCommand);

		vscode.window.onDidChangeActiveTextEditor(editor => {
			if (editor?.document.fileName) {
				updateSelectedFile(highlighters, extensionState, providers, getSelectedElements(providers, extensionState));
			}
		}, null, context.subscriptions);

		//	overkill to refresh on every change?  TODO: see if there's a performance hit; at least we want to regenerate the function list
		vscode.workspace.onDidChangeTextDocument(event => {
			const editor = vscode.window.visibleTextEditors.find(editor => editor.document.fileName === event.document.fileName);
			if (editor?.document.fileName) {
				updateSelectedFile(highlighters, extensionState, providers, getSelectedElements(providers, extensionState));
			}
		}, null, context.subscriptions);

		//	TODO
		vscode.workspace.onDidOpenTextDocument(document => {
			const editor = vscode.window.visibleTextEditors.find(editor => editor.document.fileName === document.fileName);
			if (editor?.document.fileName) {
				updateSelectedFile(highlighters, extensionState, providers, getSelectedElements(providers, extensionState));
			}
		}, null, context.subscriptions);
		//	TODO: what to do when a document is closed?

		const autotestFromEditorContextMenu = await vscode.commands.registerCommand(COMMANDS.shatterAutotestFromEditorContextMenu, async () => {
			const editor = vscode.window.activeTextEditor;
			if (editor?.document.languageId === 'typescript') {
				const selection = editor.selection;
				const cursorPosition = selection.active;
				const document = editor.document;

				const functionMeta = getFunctionNodeAtCursor(cursorPosition, document);
				if (functionMeta) {
					const functionName = functionMeta.name;
					if (!functionName) {
						throw new Error(`Top level anonymous functions are not supported`);
					}

					const absoluteFileUnderTest = document.fileName as AbsolutePath;
					providers.functionsListProvider.select(functionName);	//	select the function in the tree view, which is what everything else depends on
					await doAutotest(context, extensionState, providers, highlighters, workspaceRoots, absoluteFileUnderTest, functionName, extensionSource);
				} else {
					vscode.window.showErrorMessage('Select a function or place the cursor inside a function.');
				}
			}
		});
		context.subscriptions.push(autotestFromEditorContextMenu);

		const autotestFromFunctionViewContainerMenu = vscode.commands.registerCommand(COMMANDS.shatterAutotestFromFunctionViewContainer, (item) => {
			const selectedElements = getSelectedElements(providers, extensionState);
			if (!selectedElements.selectedFile) {
				//	TODO: error
				return;
			}
			const editor = vscode.window.visibleTextEditors.find(editor => editor.document.fileName === selectedElements.selectedFile?.filename);
			const filename = (editor?.document.fileName ?? selectedElements.selectedFile?.filename) as AbsolutePath;
			if (!filename) {
				//	TODO: is this a reasonable situation?
				return;
			}

			return doAutotest(context, extensionState, providers, highlighters, workspaceRoots, filename, item.key, extensionSource);
		});
		context.subscriptions.push(autotestFromFunctionViewContainerMenu);

		const retestCommand = await vscode.commands.registerCommand(COMMANDS.shatterRetestFromEditorContextMenu, async () => {
			console.log(`there was an attempt`);
		});
		context.subscriptions.push(retestCommand);

		const retestContextMenu = vscode.commands.registerCommand(COMMANDS.shatterRetestFromFunctionViewContainer, () => {
			console.log(`there was an attempt`);
		});
		context.subscriptions.push(retestContextMenu);

		const shatterAddTestcase = vscode.commands.registerCommand(COMMANDS.shatterAddTestcase, () => {
			console.log(`there was an attempt`);
		});
		context.subscriptions.push(shatterAddTestcase);

		const shatterResetLocalFromFunctionViewContainer = vscode.commands.registerCommand(COMMANDS.shatterResetLocalFromFunctionViewContainer, () => {
			context.workspaceState.update(autotestStorageStateKey, undefined);

			const resetStateKeys = [
				'runningTestFunction',
			] as const satisfies readonly (keyof ExtensionState)[];

			for (const k of resetStateKeys) {
				extensionState[k] = undefined;
			}
			extensionState.fileStates = {};

			for (const provider of Object.values(providers)) {
				provider.select(undefined);
			}

			initializeWorkspace(defaultWorkspaceRoot, absolutist, extensionState, 'soft');
			if (vscode.window.activeTextEditor?.document.languageId === 'typescript') {
				updateSelectedFile(highlighters, extensionState, providers, getSelectedElements(providers, extensionState));
			} else {
				refresh(getSelectedElements(providers, extensionState), extensionState, providers, highlighters);
			}
		});
		context.subscriptions.push(shatterResetLocalFromFunctionViewContainer);

		if (vscode.window.activeTextEditor?.document.languageId === 'typescript') {
			updateSelectedFile(highlighters, extensionState, providers, getSelectedElements(providers, extensionState));
		}
	} catch (e: any) {
		console.error(`Unable to load extension ${e}: ${e.stack}`);
	}
}

async function makeTestCasePersistent(baseDirectory: AbsolutePath | undefined, extensionState: ExtensionState, providers: DisplayProviders, highlighters: Record<AbsolutePath, Highlighter>, node: CommonDisplayNode) {
	//	if the test case is not persistent, save it to the location specified in the configuration
	const specimenId = node.key;
	if (!baseDirectory || !isSpecimenId(specimenId)) {
		return;
	}

	const specimental = findSpecimen(extensionState, specimenId);
	if (!specimental || specimental.specimenPath) { //	already persisted
		return;
	}

	const savePath = saveSpecimen(baseDirectory, specimental);
	specimental.specimenPath = savePath;

	refresh(getSelectedElements(providers, extensionState), extensionState, providers, highlighters);
};

const makeTestCaseNotPersistent = async (baseDirectory: AbsolutePath | undefined, extensionState: ExtensionState, providers: DisplayProviders, highlighters: Record<AbsolutePath, Highlighter>, node: CommonDisplayNode) => {
	const specimenId = node.key;
	if (!baseDirectory || !isSpecimenId(specimenId)) {
		return;
	}

	const specimental = findSpecimen(extensionState, specimenId);
	if (!specimental || !specimental.specimenPath) {	//	already not persisted
		return;
	}

	const fileUri = vscode.Uri.file(specimental.specimenPath);
	await vscode.workspace.fs.delete(fileUri);
	specimental.specimenPath = undefined;
	refresh(getSelectedElements(providers, extensionState), extensionState, providers, highlighters);
};

function initializeTreeViews(context: vscode.ExtensionContext) {
	const functionsListProvider = createTreeProvider('shatter-functions-list', context, {
		command: {
			command: COMMANDS.shatterSelectFunction,
			title: 'Functions',
		},
		rootNodeDefaultCollapseState: 'collapsed',
	});
	//	TODO: Refresh functions list view contents on change of editor
	const clustersListProvider = createTreeProvider('shatter-execution-paths', context, {
		command: {
			command: COMMANDS.shatterSelectCluster,
			title: 'Execution Paths',
		},
		rootNodeDefaultCollapseState: 'collapsed',
	});

	const testCaseListProvider = createTreeProvider('shatter-list-testcases', context, {
		command: {
			command: COMMANDS.shatterSelectTestCase,
			title: 'Test Case Detail',
		},
		stateIcons: iconPaths(context, { pinned: 'pin.svg', unpinned: 'unpin.svg', edge: 'sparkle.svg' }),
	});

	const testCaseDetailProvider = createTreeProvider("shatter-testcase-detail", context, {
		stateIcons: iconPaths(context, { persistent: 'pin.svg' }),
	});

	const providers = {
		functionsListProvider,
		clustersListProvider,
		testCaseListProvider,
		testCaseDetailProvider,
	};
	return providers;
}

async function initializeWorkspace(defaultWorkspaceRoot: AbsolutePath, absolutist: (filename: RelativePath) => AbsolutePath, extensionState: ExtensionState, load: 'hard' | 'soft'): Promise<ProjectConfiguration> {
	if (!defaultWorkspaceRoot) {
		return {};
	}

	const configuration: ProjectConfiguration = await readProjectConfiguration(defaultWorkspaceRoot);
	if (!configuration.baseDirectory) {
		return configuration;;
	}

	if (load === 'hard') {
		initializeWorkspaceWatchers(configuration, defaultWorkspaceRoot, absolutist, extensionState);
	}

	//	do this *after* the watcher is set up to avoid missing any additions
	//	TODO: might miss some deletions
	const absoluteBaseDirectory = joinAbsolute(defaultWorkspaceRoot, configuration.baseDirectory);
	const initialPersistentSpecimens = await loadPersistedSpecimens(absolutist, absoluteBaseDirectory);
	initialPersistentSpecimens.forEach((specimental, id) => {
		onPersistedSpecimenLoad(absolutist, extensionState, specimental.specimen, id, specimental.specimenPath);
	});

	const expected = await loadExpected(absoluteBaseDirectory);
	extensionState.expected = expected;

	return configuration;
}

function initializeWorkspaceWatchers(configuration: ProjectConfiguration, defaultWorkspaceRoot: string, absolutist: (filename: RelativePath) => AbsolutePath, extensionState: ExtensionState) {
	const ignoreCreate = false;
	const ignoreChange = true;
	const ignoreDelete = false;

	//	NOTE: this watcher is using a different API than listPersistedSpecimens because
	//	the latter is meant to be independent of VS Code
	const watcher = vscode.workspace.createFileSystemWatcher(`${configuration.baseDirectory}/**/*.json`, ignoreCreate, ignoreChange, ignoreDelete);

	watcher.onDidCreate((e) => {
		const absoluteSpecimenFilepath = e.fsPath as AbsolutePath;
		const maybeSpecimenId = path.basename(absoluteSpecimenFilepath).substring(0, '.json'.length);
		if (isSpecimenId(maybeSpecimenId)) {
			const specimen = loadPersistedSpecimen(absoluteSpecimenFilepath as AbsolutePath);

			if (!defaultWorkspaceRoot) {
				throw new Error(`Unexpectedly no workspace root for ${absoluteSpecimenFilepath}`);
			}

			onPersistedSpecimenLoad(absolutist, extensionState, specimen, maybeSpecimenId, absoluteSpecimenFilepath);
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
}

function iconPaths(context: vscode.ExtensionContext, baseSet: Record<string, string>) {
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

type CommonTreeDataProviderOptions = {
	command?: Pick<vscode.Command, 'command' | 'title'>,
	stateIcons?: Record<string, Record<'dark' | 'light', string>>,
	rootNodeDefaultCollapseState?: 'expanded' | 'collapsed',
};

function createTreeProvider(viewName: string, context: vscode.ExtensionContext, options?: CommonTreeDataProviderOptions) {
	const treeDataProvider = new CommonTreeDataProvider(options);
	const treeView = vscode.window.createTreeView(viewName, { treeDataProvider });
	treeDataProvider.treeView = treeView;
	context.subscriptions.push(treeView);
	return treeDataProvider;
}

export function deactivate() { }

// Define a custom TreeDataProvider for the result clusters
class CommonTreeDataProvider implements vscode.TreeDataProvider<CommonDisplayNode>, DisplayProvider {
	private _onDidChangeTreeData: vscode.EventEmitter<CommonDisplayNode | undefined | void> = new vscode.EventEmitter<CommonDisplayNode | undefined>();
	readonly onDidChangeTreeData: vscode.Event<CommonDisplayNode | undefined | void> = this._onDidChangeTreeData.event;

	private roots: CommonDisplayNode[] | undefined;
	treeView: vscode.TreeView<CommonDisplayNode> | undefined;

	// Initialize empty
	constructor(private options?: CommonTreeDataProviderOptions) {
		this.roots = undefined;
	}

	// update notify the tree view.
	//	TODO: if the tree provider is going to know about AutotestResults
	//	then it should do the conversion also
	refresh(roots: CommonDisplayNode[] | undefined) {
		this.roots = roots;
		this._onDidChangeTreeData.fire();
	}

	select(key: string | undefined): void {
		if (this.treeView && this.roots) {
			if (key) {

				const item = findNode(this.roots, key);
				if (item) {
					// this.treeView.reveal(item, { select: true });
				}
			} else {
				//	TODO: unselect https://github.com/microsoft/vscode/issues/48754
				this.treeView.reveal({ label: '', key: '' }, { select: false, focus: false });	//	maybe this unselects?
			}
		}
	}

	getSelected(): readonly CommonDisplayNode[] {
		return this.treeView?.selection ?? [];
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
		//	TODO: creating a new TreeItem on every fetch blows away the collapsed state; need each CommonDisplayNode to have a unique ID and then cache the TreeItems
		const treeItem = new vscode.TreeItem(element.label);

		const defaultCollapseState = this.options?.rootNodeDefaultCollapseState === 'collapsed'
			? vscode.TreeItemCollapsibleState.Collapsed
			: vscode.TreeItemCollapsibleState.Expanded;
		treeItem.collapsibleState = element.children && element.children.length > 0
			? defaultCollapseState
			: vscode.TreeItemCollapsibleState.None;
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
		// treeItem.checkboxState = vscode.TreeItemCheckboxState.Checked;
		return treeItem;
	}
}
