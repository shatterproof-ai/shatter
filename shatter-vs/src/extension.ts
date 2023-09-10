import * as fs from 'fs'; //TODO: use VSCode fs
import * as path from 'path';
import * as ts from 'typescript';
import * as vscode from 'vscode';
import { ResultCluster, shatterAutotest } from './shatter';
import { RunResult } from './supervisor';
import { Cluster } from 'cluster';
import { join } from 'path';

interface ClusterNode {
	label: string;
	children?: ClusterNode[];
}

function runResultToClusterNode(prefix: string, result: RunResult): ClusterNode {
	const strung = JSON.stringify(result.parameters)
	const maxlength = 35;
	return {
		//	TODO: convert the parameter list into nodes
		label: `${prefix} (${result.duration}ms), input = ${strung.substring(0, maxlength)}${strung.length > maxlength ? '...' : ''}`,
		children: clusterValues(result.parameters),
	}
}

function visit(k: string | number, o: any, depth=0): ClusterNode {
	if (depth == 0) {
		return {
			label: "...",
		}
	}

	const key = typeof k == 'number' ? `[${k}]` : `"${k}"`
	if (o === null) {
		return {
			label: `${key}: null`,
		}
	}
	if (o === undefined) {
		return {
			label: `${key}: undefined`,
		}
	}
	if (typeof o === 'object') {
		if (Array.isArray(o)) {
			return {
				label: key,
				children: o.map((v, i) => visit(i, v, depth - 1)),
			}
		}
		const keys = Object.keys(o)
		const children = keys.map((k) => visit(k, o[k], depth - 1))
		return {
			label: key,
			children,
		}
	}
	return {
		label: `${key}: ${o}`,
	}
}

function clusterValues(params: any[]): ClusterNode[] {
	const nodes: ClusterNode[] = params.map((p, i) => visit(i, p, 3))
	return nodes
}

function createClusterNodes(clusters: ResultCluster[]): ClusterNode[] {
	const nf = Intl.NumberFormat("en-US", {
		style: 'decimal',
		maximumSignificantDigits: 3,
	})

	const clusterNodes: ClusterNode[] = clusters.map((cluster) => {
		const children: ClusterNode[] = [
			{
				label: `${cluster.results.length} attempts, average ${nf.format(cluster.totalTime / cluster.results.length)}ms`,
			},
		]

		const step = Math.max(1, Math.floor(cluster.results.length / 10))
		//	get a node at the start then approximately each decile (if at least 10) but definitely not the last one
		for (let i = 0; i < cluster.results.length - 2; i += step) {
			children.push(runResultToClusterNode(`p${Math.round(100 * i / cluster.results.length)}`, cluster.results[i]))
		}
		children.push(runResultToClusterNode(`p100`, cluster.results[cluster.results.length - 1]))

		const label = `${cluster.key.substring(0, 6)}: ${cluster.outcome} (${cluster.results.length} trials)`
		const clusterNode: ClusterNode = {
			label,
			children,
		};

		return clusterNode;
	})
	return clusterNodes
}

export function activate(context: vscode.ExtensionContext) {
	const astDataProvider = new ASTTreeDataProvider();
	vscode.window.registerTreeDataProvider('shatterResultsView', astDataProvider);

	//	TODO: fix the ugly hard-coding of 'src'; that can't be right for a standalone extension
	//	TODO: just make people import shatterproof module in their projects; don't try to be magical about it
	//	shatterproof needs an existence outside VSCode anyway
	const extensionSource = join(context.extensionPath, 'src');

	const disposable = vscode.commands.registerCommand('extension.shatterAutotest', async () => {
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
					const allTsConfigs: string[] = [];
					const allPackageJsons: string[] = [];
					const allNodeModules: string[] = [];
					const allWorkspaceFolders: string[] = [];

					['shatter', 'shatter-vs', 'shatterproof', 'shatterproof-vs', context.extension.id].forEach((confkey) => {
						console.log(`configuration ${confkey} = ${JSON.stringify(vscode.workspace.getConfiguration(confkey))}`)
					})

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

					// throw new Error("Need to figure out how to find module 'shatterproof'")
					const modulePaths = [...allWorkspaceFolders, ...allNodeModules];

					await shatterAutotest(modulePaths,
						functionNode.getSourceFile().fileName,
						functionNode.getText(), (clusters) => {
							const treeNodes = createClusterNodes(clusters)

							console.log(`refreshing function node to display = ${functionNode.name?.text}`);
							astDataProvider.refresh(treeNodes);
						}, extensionSource);

				} else {
					console.log(`function node not found`);
				}
			} else {
				vscode.window.showErrorMessage('Select a function or place the cursor inside a function.');
			}
		}
	});

	context.subscriptions.push(disposable);

	const disposableContextMenu = vscode.commands.registerCommand('extension.shatterAutotestContext', () => {
		vscode.commands.executeCommand('extension.shatterAutotest');
	});

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

	context.subscriptions.push(disposableContextMenu);
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


// Define a custom TreeDataProvider for the AST.
class ASTTreeDataProvider implements vscode.TreeDataProvider<ClusterNode> {
	private _onDidChangeTreeData: vscode.EventEmitter<ClusterNode | undefined | void> = new vscode.EventEmitter<ClusterNode | undefined>();
	readonly onDidChangeTreeData: vscode.Event<ClusterNode | undefined | void> = this._onDidChangeTreeData.event;

	private roots: ClusterNode[] | undefined;

	// Initialize empty
	constructor() {
		this.roots = undefined;
	}

	// Refresh the AST and notify the tree view.
	refresh(roots: ClusterNode[] | undefined) {
		this.roots = roots;

		console.log(`firing onchange with ${JSON.stringify(roots)}}`);

		this._onDidChangeTreeData.fire();
	}

	// Get the children of a tree node.
	getChildren(element?: ClusterNode): Thenable<ClusterNode[]> {
		if (!element) {
			// Return the root nodes if element is undefined as that indicates the beginning of traversal
			return Promise.resolve(this.roots ? this.roots : []);
		}
		const children = element.children || [];
		return Promise.resolve(children);
	}

	// Get the parent of a tree node.
	getParent(element: ClusterNode): ClusterNode | null {
		return null; // We're not using parent-child relationships.
	}

	// Get the tree item for a node.
	getTreeItem(element: ClusterNode): vscode.TreeItem {
		const treeItem = new vscode.TreeItem(element.label);
		treeItem.collapsibleState = element.children ? vscode.TreeItemCollapsibleState.Collapsed : vscode.TreeItemCollapsibleState.None;
		//	TODO: tooltip should be expanded (but still bounded) parameter list
		treeItem.tooltip = element.label;
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