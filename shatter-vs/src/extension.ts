import * as vscode from 'vscode';
import * as ts from 'typescript';

export function activate(context: vscode.ExtensionContext) {
	console.log(`activationing`)
	const astDataProvider = new ASTTreeDataProvider();
	vscode.window.registerTreeDataProvider('shatterResultsView', astDataProvider);

	const disposable = vscode.commands.registerCommand('extension.shatterAutotest', () => {
		const editor = vscode.window.activeTextEditor;

		console.log(`languageId = ${editor?.document.languageId}`)

		if (editor && editor.document.languageId === 'typescript') {
			const selection = editor.selection;
			const cursorPosition = selection.active;
			const document = editor.document;

			console.log(`cursorPosition = ${cursorPosition.line} ${cursorPosition.character}`)
			if (isCursorInFunctionName(cursorPosition, document, editor)) {
				const functionNode = getFunctionNodeAtCursor(cursorPosition, document);

				if (functionNode && ts.isFunctionDeclaration(functionNode)) {

					const astNode = createASTNode(functionNode);
					console.log(`refreshing function node to display = ${functionNode.name?.text}`)
					astDataProvider.refresh(astNode);
				} else {
					console.log(`function node not found`)
				}
			} else {
				vscode.window.showErrorMessage('Select a function or place the cursor inside a function.');
			}
		}
	});

	context.subscriptions.push(disposable);

	const disposableContextMenu = vscode.commands.registerCommand('extension.shatterAutotestContext', () => {
		console.log(`extension.shatterAutotestContext command registered`)
		vscode.commands.executeCommand('extension.shatterAutotest');
	});

	vscode.languages.registerCodeActionsProvider(
		{ scheme: 'file', language: 'typescript' },
		{
			provideCodeActions: (document, range) => {
				console.log(`provideCodeActions called`)
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

	console.log(`sourceFile = ${sourceFile}`)

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

// Define a data structure to represent AST nodes.
interface ASTNode {
	label: string;
	kind: ts.SyntaxKind;
	line: number;
	children?: ASTNode[];
}

// Define a custom TreeDataProvider for the AST.
class ASTTreeDataProvider implements vscode.TreeDataProvider<ASTNode> {
	private _onDidChangeTreeData: vscode.EventEmitter<ASTNode | undefined | void> = new vscode.EventEmitter<ASTNode | undefined>();
	readonly onDidChangeTreeData: vscode.Event<ASTNode | undefined | void> = this._onDidChangeTreeData.event;

	private ast: ASTNode | undefined;

	// Initialize with an empty AST.
	constructor() {
		this.ast = undefined;
	}

	// Refresh the AST and notify the tree view.
	refresh(ast: ASTNode | undefined) {
		this.ast = ast;

		console.log(`firing onchange with ${JSON.stringify(ast)}}`)

		this._onDidChangeTreeData.fire();
	}

	// Get the children of a tree node.
	getChildren(element?: ASTNode): Thenable<ASTNode[]> {
		if (!element) {
			console.log(`element is undefined; returning root, which has ${this.ast?.children?.length} children`)
			// Return the root node if element is undefined.
			return Promise.resolve(this.ast ? [this.ast] : []);
		}
		const children = element.children || []
		console.log(`returning children of ${element.label} = ${children.length}`)
		return Promise.resolve(children);
	}

	// Get the parent of a tree node.
	getParent(element: ASTNode): ASTNode | null {
		console.log(`getParent called for ${element.label}`)
		return null; // We're not using parent-child relationships.
	}

	// Get the tree item for a node.
	getTreeItem(element: ASTNode): vscode.TreeItem {
		console.log(`getTreeItem called for ${element.label}`)
		const treeItem = new vscode.TreeItem(element.label);
		treeItem.collapsibleState = element.children ? vscode.TreeItemCollapsibleState.Collapsed : vscode.TreeItemCollapsibleState.None;
		treeItem.tooltip = `Line ${element.line}`;
		return treeItem;
	}
}

function createASTNode(node: ts.Node): ASTNode {

	const start = ts.getLineAndCharacterOfPosition(node.getSourceFile(), node.getStart())
	const end = ts.getLineAndCharacterOfPosition(node.getSourceFile(), node.getEnd())

	const text = node?.getChildren()?.length == 0 ? `: ${node.getText()}` : ''

	const label = `${ts.SyntaxKind[node.kind]}:${start.line + 1}:${start.character} - ${end.line + 1}-${end.character}${text}`

	return {
		label,
		kind: node.kind,
		line: start.line + 1,
		children: node.getChildren().map(createASTNode),
	};
}