import * as fs from 'fs'; //TODO: use VSCode fs
import * as path from 'path';
import { join } from 'path';
import * as ts from 'typescript';
import * as vscode from 'vscode';
import { AbsolutePath, RelativePath, SpecimenId, isRelativePath, isSpecimenId } from '../core/common';
import { FunctionMeta } from '../core/transform';
import { CoverageSelection, ExtensionState, Specimental, cleanUpExtensionState, getActiveStates, onPersistedSpecimenLoad } from './common';
import { CommonDisplayNode, DisplayProvider, Highlighter, doSelectCluster, doSelectFile, doSelectFunction, doSelectTestCase, refresh } from './display';
import { forkTest, loadPersistedSpecimen, loadPersistedSpecimens, saveTest } from './persistence';
import { TestLifecycle, autotestFunction } from './run';
import { Outcome, Outcomes, isOutcome } from '../core/supervisor';

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

/*
Operations:
* open test case
* save test case
* add test case

Provide context menu for running a test case from a file

TODO: convert the test case tree view into a test case  manager

How to select test cases?  Per function, per cluster, per test case

*/

function highlighterFromEditor(): Highlighter {
	const editor = vscode.window.activeTextEditor;
	if (editor) {
		editor.setDecorations(coveredDecorationType, []);
		editor.setDecorations(missedDecorationType, []);
		function doHighlighting(decoration: 'covered' | 'missed', linerator: () => Generator<number, void, unknown>) {

			const decorationType = decoration === 'missed' ? missedDecorationType : coveredDecorationType;

			//	TODO: replace with function pointer or pubsub or something that doesn't require passing around the editor object
			highlightLinesInEditor(editor, decorationType, linerator());
		}
		return doHighlighting;
	}

	//	nothing to do
	return () => { };
}

const autotestStorageStateKey = "autotestState_0";
export async function activate(context: vscode.ExtensionContext) {
	//	TODO: this all needs to deal in URIs
	const workspaceRoots: AbsolutePath[] = vscode.workspace.workspaceFolders?.map((f) => f.uri.fsPath as AbsolutePath) ?? [];
	const defaultWorkspaceRoot: AbsolutePath | undefined = workspaceRoots[0];
	let configuration: ProjectConfiguration = {};
	let specimenBaseDirectory: AbsolutePath | undefined = undefined;

	const extensionState: ExtensionState = cleanUpExtensionState(context.workspaceState.get(autotestStorageStateKey, {}));

	const highlighter = highlighterFromEditor();

	const absolutist = (filename: RelativePath): AbsolutePath => {
		if (!defaultWorkspaceRoot) {
			throw new Error(`Unexpectedly no workspace root for ${filename}`);
		}
		return asAbsolutePath(defaultWorkspaceRoot, filename);
	};

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

				//	do this *after* the watcher is set up to avoid missing any additions
				//	TODO: might miss some deletions
				specimenBaseDirectory = asAbsolutePath(defaultWorkspaceRoot, configuration.testsDirectory);
				const initialPersistentSpecimens = loadPersistedSpecimens(absolutist, specimenBaseDirectory);
				initialPersistentSpecimens.forEach((specimental, id) => {
					onPersistedSpecimenLoad(absolutist, extensionState, specimental.specimen, id, specimental.specimenPath);
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
				doSelectFile(highlighter, extensionState, filename as AbsolutePath, providers);
			}
		};

		//	call after switching files, changing contents of the editor, or running tests
		const doSelectFunctionCommand = (node: CommonDisplayNode) => {
			if (vscode.window.activeTextEditor?.document.languageId === 'typescript') {
				const functionName: string = node.key || "";
				doSelectFunction(highlighter, extensionState, providers, functionName);
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
						if (isOutcome(node.key)) {
							return node.key;
						}

						throw new Error(`unhandled key ${node.key}`);
					}
				})();
				if (selection) {
					doSelectCluster(highlighter, extensionState, providers, selection);
				}
			}
		};

		const doSelectTestCaseCommand = (node: CommonDisplayNode) => {
			if (vscode.window.activeTextEditor?.document.languageId === 'typescript') {
				const specimenId: string = node.key || "";
				doSelectTestCase(highlighter, extensionState, providers, specimenId);
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
			refresh(extensionState, providers, highlighter);
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
			refresh(extensionState, providers, highlighter);
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

			const testCaseNamePattern = /^[a-z0-9_-.]+$/;
			function isValidTestCaseName(s:string|undefined) {
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
			if (newId in functionState.specimens) {
				//TODO: error
				return;
			}

			//	if persistable and the base test is already persisted
			if (specimenBaseDirectory) {
				// function forkTest(storageBaseDirectory: AbsolutePath, specimental: Specimental, sourceFileUnderTestPath: RelativePath, testCaseName: SpecimenId) {


				let newSpecimental: Specimental | undefined = undefined;
				if (specimental.fileUnderTest) {
					//	forking a persistent test
					newSpecimental = forkTest(specimenBaseDirectory, specimental, newId, newTestCaseName);
					functionState.specimens[newId] = {
						...specimental,
						clusterKey: specimental.clusterKey,
						fileUnderTest: specimental.fileUnderTest,
						specimen: specimental.specimen,
					};
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
					const specimenFileAbsolutePath = saveTest(specimenBaseDirectory, newSpecimental);
					newSpecimental.specimenPath = specimenFileAbsolutePath;
				}

				if (vscode.window.activeTextEditor?.document.languageId === 'typescript') {
					refresh(extensionState, providers, highlighter);
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
					await doAutotest(absoluteFileUnderTest, functionName);
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

			doAutotest(filename as AbsolutePath, item.key);
		});
		context.subscriptions.push(autotestFromFunctionViewContainerMenu);

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

		if (vscode.window.activeTextEditor?.document.languageId === 'typescript') {
			updateSelectedFile();
		}

		//	TODO: some sort of status display during execution
		//	TODO: show the sidebar when running
		async function doAutotest(absoluteSourceFilename: AbsolutePath, functionName: string) {
			const editor = vscode.window.activeTextEditor;
			if (editor?.document.languageId !== 'typescript') {
				return;
			}

			const lifeCycler: TestLifecycle = {
				onTestStart(absoluteFilename: AbsolutePath, functionName: string) {
					doSelectFunctionCommand({
						key: functionName,
						label: ''
					});
				},

				onResult(absoluteFilename, functionName, result) {
					refresh(extensionState, providers, highlighter);
				},

				onTestEnd(absoluteFilename: AbsolutePath, functionName: string) {
					context.workspaceState.update(autotestStorageStateKey, extensionState);
					extensionState.runningAutotestFunction = undefined;
					refresh(extensionState, providers, highlighter);
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
	} catch (e: any) {
		console.error(`Unable to load extension ${e}: ${e.stack}`);
	}
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
class CommonTreeDataProvider implements vscode.TreeDataProvider<CommonDisplayNode>, DisplayProvider {
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
