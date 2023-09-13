import * as fs from 'fs'; //TODO: use VSCode fs
import * as path from 'path';
import { join } from 'path';
import * as ts from 'typescript';
import * as vscode from 'vscode';
import { AutotestResults, shatterAutotest } from './shatter';
import { RunResult } from './supervisor';
import { findFunctions } from './transform';

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

const clusterValues = (values: any[]) =>
	values.map((value, i) => visit(i, value, 3));

function runResultToClusterNode(prefix: string, result: RunResult): CommonDisplayNode {
	const resultChildren: CommonDisplayNode[] = [];
	if (result.output) {
		resultChildren.push(
			visit("output", result.output, 3));
	}

	if (result.error) {
		resultChildren.push(
			visit("error", result.error, 3));
	}

	return {
		label: prefix,
		children: [{
			label: "Duration",
			children: [{
				label: `${result.duration}ms`
			}]
		}, {
			label: "Parameters",
			children: clusterValues(result.parameters)
		}, {
			label: "Result",
			children: resultChildren
		}],
	};
}

type FunctionState = {
	autotest: AutotestResults;
};

type FileState = {
	functions: ts.FunctionDeclaration[];
	functionStates: Record<string, FunctionState>;
};

type ExtensionState = {
	fileStates: Record<string, FileState>
	activeFile?: string;
	activeFunction?: string;
	activeClusterKey?: string;
};

interface Providers {
	functionsListProvider: CommonTreeDataProvider,
	clustersListProvider: CommonTreeDataProvider,
	coverageProvider: CommonTreeDataProvider,
	testCasesProvider: CommonTreeDataProvider,
}

const decorationType = vscode.window.createTextEditorDecorationType({
	// gutterIconPath: context.asAbsolutePath('media/triangle.svg'),
	//	TODO: get colors from theme and/or IDE https://code.visualstudio.com/api/references/theme-color#text-colors
	light: {
		backgroundColor: 'lightgray',
	},
	dark: {
		backgroundColor: 'dimgray',
	},
});

function updateDecorations(editor: vscode.TextEditor, extensionState: ExtensionState, fileState: FileState) {
	const text = editor.document.getText();
	const decorationsArray: vscode.DecorationOptions[] = [];

	if (!extensionState.activeFunction || !extensionState.activeClusterKey) {
		editor.setDecorations(decorationType, []);
		return;
	}
	const functionState = fileState.functionStates[extensionState.activeFunction];
	if (!functionState) {
		//	TODO: should not happen
		editor.setDecorations(decorationType, []);
		return;
	}

	const activeCluster = functionState.autotest.clusters.find((cluster) => cluster.key === extensionState.activeClusterKey);
	if (!activeCluster) {
		editor.setDecorations(decorationType, []);
		return;
	}

	console.log(`updateDecorations for active cluster = ${activeCluster.key}`);

	if (activeCluster) {
		const linesToHighlight: number[] = [];
		activeCluster.branches.forEach((branchName) => {
			const branch = functionState.autotest.branches.get(branchName);
			if (branch) {
				linesToHighlight.push(branch.line);
			} else {
				//	well this is pretty weird
			}
		});

		linesToHighlight.forEach(lineNumber => {
			const line = editor.document.lineAt(lineNumber);
			const decoration = { range: line.range, hoverMessage: `Line ${lineNumber}: ${line.text}` };
			decorationsArray.push(decoration);
		});

		editor.setDecorations(decorationType, decorationsArray);
	} else {
		//	TODO: logic for removing decorations
		editor.setDecorations(decorationType, []);
	}
}

const refresh = (editor: vscode.TextEditor | undefined, extensionState: ExtensionState, providers: Providers) => {
	const { functionsListProvider, clustersListProvider, coverageProvider, testCasesProvider } = providers;

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

	const nodes: CommonDisplayNode[] = fileState.functions.map((f) => ({
		label: f.name?.text || "",
		key: f.name?.text || "",
	}));

	functionsListProvider.refresh(nodes);

	if (!extensionState.activeFunction) {
		return;
	}

	const func = fileState.functions.find((f) => f.name?.text === extensionState.activeFunction);
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

	const clusterNodes: CommonDisplayNode[] = results.clusters.map((cluster) => {
		//	TODO: list each trial as a child node with duration, completion state, truncated stringified parameter list, and truncated output
		const key = cluster.key.substring(0, 6);
		return {
			label: `${key}: ${cluster.outcome} (${cluster.results.length} trials)`,
			key: cluster.key,
		};
	});
	clustersListProvider.refresh(clusterNodes);

	if (!extensionState.activeClusterKey) {
		return;
	}

	const selectedCluster = results.clusters.find((cluster) => cluster.key === extensionState.activeClusterKey);
	if (!selectedCluster) {
		return;
	}

	if (editor) {
		//	TODO: replace with function pointer or pubsub or something that doesn't require passing around the editor object
		updateDecorations(editor, extensionState, fileState);
	}

	const coverageNodes: CommonDisplayNode[] = selectedCluster.branches.map((branchName) => {
		const branch = results.branches.get(branchName);
		if (!branch) {
			throw new Error(`Could not find branch ${branchName}`);
		}

		return {
			label: `${branchName}: line ${branch.line}`,
			children: [],
			key: branch.id,
		};
	});
	coverageProvider.refresh(coverageNodes);

	const testCasesNodes: CommonDisplayNode[] = selectedCluster.results.map((result, i) =>
		//	TODO: show just inputs and outputs
		runResultToClusterNode(`[${i}]`, result)
	);
	testCasesProvider.refresh(testCasesNodes);
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

	const selectedFunction = filestate.functions.find((f) => f.name?.text === functionName);
	if (selectedFunction) {
		extensionState.activeFunction = functionName;
	} else {
		extensionState.activeClusterKey = undefined;
		extensionState.activeFunction = undefined;
	}
	refresh(editor, extensionState, providers);
};

const doSelectCluster = (editor: vscode.TextEditor, extensionState: ExtensionState, providers: Providers, clusterKey: string) => {
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

	const selectedFunction = functions.find((f) => f.name?.text === extensionState.activeFunction);
	if (!selectedFunction) {
		//	TODO: shouldn't happen
		return;
	}

	const functionState = filestate.functionStates[extensionState.activeFunction];
	if (!functionState) {
		//	TODO: shouldn't happen
		return;
	}

	const cluster = functionState.autotest.clusters.find((cluster) => cluster.key === clusterKey);
	if (cluster) {
		extensionState.activeClusterKey = clusterKey;
		refresh(editor, extensionState, providers);
	}
};


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
		title: 'Functions',
	});
	context.subscriptions.push(
		vscode.window.registerTreeDataProvider("shatter-execution-paths", clustersListProvider));

	const coverageProvider = new CommonTreeDataProvider();
	context.subscriptions.push(
		vscode.window.registerTreeDataProvider("shatter-coverage", coverageProvider));

	const testCasesProvider = new CommonTreeDataProvider();
	context.subscriptions.push(
		vscode.window.registerTreeDataProvider("shatter-test-cases", testCasesProvider));

	const providers = {
		functionsListProvider,
		clustersListProvider,
		coverageProvider,
		testCasesProvider,
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
			const clusterKey: string = node.key || "";
			console.log(`doSelectCluster called with ${clusterKey} from ${JSON.stringify(node)}, activeFile ${extensionState.activeFile}, and activeFunction ${extensionState.activeFunction}`);
			doSelectCluster(vscode.window.activeTextEditor, extensionState, providers, clusterKey);
		}
	};

	//	called by the command handler for the function selector
	//	needs to be registered as a command because TreeView needs a command to dispatch to
	const selectFunctionCommand = vscode.commands.registerCommand('extension.shatterSelectFunction', doSelectFunctionCommand);
	context.subscriptions.push(selectFunctionCommand);

	//	needs to be registered as a command because TreeView needs a command to dispatch to
	const selectClusterCommand = vscode.commands.registerCommand('extension.shatterSelectCluster', doSelectClusterCommand);
	context.subscriptions.push(selectClusterCommand);

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
				const functionNode = getFunctionNodeAtCursor(cursorPosition, document);

				if (functionNode && ts.isFunctionDeclaration(functionNode)) {
					const functionName = functionNode.name?.text;
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

		console.log("BEGIN THE AUTOTEST");
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
			}, extensionSource);
		console.log("END THE AUTOTEST");
	}
}

function doSelectFile(editor: vscode.TextEditor | undefined, extensionState: ExtensionState, filename: string, providers: { functionsListProvider: CommonTreeDataProvider; clustersListProvider: CommonTreeDataProvider; coverageProvider: CommonTreeDataProvider; testCasesProvider: CommonTreeDataProvider; }) {
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

function getFunctionNodeAtCursor(cursorPosition: vscode.Position, document: vscode.TextDocument): ts.Node | undefined {
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
	return findFunction(sourceFile);
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