import * as vscode from 'vscode';
import * as ts from 'typescript';

export function activate(context: vscode.ExtensionContext) {
	console.log(`activationing`)
	const disposable = vscode.commands.registerCommand('extension.shatterAutotest', () => {
		const editor = vscode.window.activeTextEditor;

		console.log(`languageId = ${editor?.document.languageId}`)

		if (editor && editor.document.languageId === 'typescript') {
			const selection = editor.selection;
			const cursorPosition = selection.active;
			const document = editor.document;

			console.log(`cursorPosition = ${cursorPosition}`)
			if (isCursorInFunctionName(cursorPosition, document, editor)) {
				console.log(`cursorPosition is in function name = ${cursorPosition}`)
				const functionNode = getFunctionNodeAtCursor(cursorPosition, document);
				
				if (functionNode) {
					console.log(`function node to display = ${cursorPosition}`)
					const panel = vscode.window.createWebviewPanel(
						'shatterAutotest',
						'Shatter Autotest',
						vscode.ViewColumn.Two,
						{}
					);

					const astTreeView = new ASTTreeView(panel, functionNode);
					astTreeView.render();
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

class ASTTreeView {
	private panel: vscode.WebviewPanel;
	private functionNode: ts.Node;

	constructor(panel: vscode.WebviewPanel, functionNode: ts.Node) {
		this.panel = panel;
		this.functionNode = functionNode;
	}

	render() {
		console.log(`rendering AST tree view`)
		const htmlContent = this.generateHTML();
		this.panel.webview.html = htmlContent;
	}

	private generateHTML(): string {
		const rootNode = this.createNode(this.functionNode);
		return `
      <!DOCTYPE html>
      <html>
      <head>
        <title>AST Tree View</title>
        <style>
          ul {
            list-style-type: none;
            padding-left: 20px;
          }
        </style>
      </head>
      <body>
        <h2>AST Tree View</h2>
        <ul>${rootNode}</ul>
      </body>
      </html>`;
	}

	private createNode(node: ts.Node): string {
		const nodeType = ts.SyntaxKind[node.kind];
		const nodeText = node.getText();

		// Get the line and character positions of the start and end positions.
		const start = ts.getLineAndCharacterOfPosition(node.getSourceFile(), node.getStart());
		const startLine = start.line + 1;
		const startChar = start.character + 1;

		const endLineAndChar = ts.getLineAndCharacterOfPosition(node.getSourceFile(), node.getEnd());
		const endLine = endLineAndChar.line + 1;
		const endChar = endLineAndChar.character + 1;

		const lineInfo = (startLine === endLine)
			? `${startLine}: ${startChar}-${endChar}`
			: `${startLine}: ${startChar} - ${endLine}: ${endChar}`;

		let result = `<li>${nodeType}: ${nodeText} (${lineInfo})`;

		if (node.getChildCount() > 0) {
			result += '<ul>';
			node.forEachChild((child) => {
				result += this.createNode(child);
			});
			result += '</ul>';
		}

		result += '</li>';
		return result;
	}
}

export function deactivate() { }
