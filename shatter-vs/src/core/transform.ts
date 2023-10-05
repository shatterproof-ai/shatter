import * as ts from 'typescript';
import { Node, SourceFile } from 'typescript';

import { FunctionDeclaration } from 'typescript';
import { SourceMapGenerator } from 'source-map';
import { AbsolutePath } from './common';

const SP_FAKE_KEY = "__sp_fake";
const SP_ORIGINAL_KEY = "__sp_original";

const c = <N extends ts.Node>(n: N, original: ts.Node | null): N => {
    (n as any)[SP_FAKE_KEY] = 1;
    (n as any)[SP_ORIGINAL_KEY] = original;
    return n;
};

const createImportStatement = (factory: ts.NodeFactory, module: string, ...things: { name: string, alias?: string }[]) => {
    const imports: ts.ImportSpecifier[] = [];

    for (const { name, alias } of things) {
        imports.push(
            factory.createImportSpecifier(false,
                factory.createIdentifier(name),
                factory.createIdentifier(alias ?? name),
            )
        );
    }

    return factory.createImportDeclaration(
        undefined,
        factory.createImportClause(
            false,
            undefined,
            factory.createNamedImports(imports)
        ),
        factory.createStringLiteral(module),
        undefined
    );
};

export type IntrospectionContext = {
    exported: Set<string>,
    functions: Map<string, FunctionDeclaration>,
    instrumentedLines: Set<number>,
    strings: Set<string>,
    numbers: Set<number>,
};

const thenCanReturn = (node: ts.Statement | ts.Block): boolean => {
    if (ts.isBlock(node)) {
        return !!node.statements.find(thenCanReturn);
    }
    return ts.isReturnStatement(node) || ts.isThrowStatement(node);
};

export interface FunctionMeta {
    name: string;
    startLine: number;
    endLine: number;
}

export const findFunctions = (sourceFileName: AbsolutePath): FunctionMeta[] => {
    const functions: FunctionMeta[] = [];
    const exportedFunctions = new Set<string>();

    const program = ts.createProgram([sourceFileName], {});
    const sourceFile = program.getSourceFile(sourceFileName);
    if (!sourceFile) {
        throw new Error(`Could not find source file ${sourceFileName}`);
    }

    const findFunctionsVisitor = (node: Node): Node => {
        if (ts.isSourceFile(node)) {
            node.statements.forEach(statement => ts.visitNode(statement, findFunctionsVisitor));
            return node;
        }

        //  declared functions only to start
        if (ts.isFunctionDeclaration(node) && node.name) {
            if (node.modifiers && node.modifiers.find(modifier => modifier.kind === ts.SyntaxKind.ExportKeyword)) {
                exportedFunctions.add(node.name.text);
            }
            functions.push({
                name: node.name.text,
                startLine: ts.getLineAndCharacterOfPosition(sourceFile, node.pos).line,
                endLine: ts.getLineAndCharacterOfPosition(sourceFile, node.end).line,
            });

            //  do not recurse into functions; we only care about top level
        }

        if (ts.isArrowFunction(node)) {
            if (ts.isBinaryExpression(node.parent)) {
                if (ts.isIdentifier(node.parent.left)) {
                    //  TODO
                    //  possibly of form let x = () => 5;
                }
            }
        }

        return node;
    };

    ts.visitNode(sourceFile, findFunctionsVisitor);
    return functions;
};


//  TODO: replace all of this with something off the shelf e.g. Istanbul or Babel
//  https://github.com/istanbuljs/istanbuljs/tree/master/packages/istanbul-lib-instrument
//  TODO: instrument every line because every line could throw an exception and thus be a branch
//  TODO: take note of throws clauses in the tested code to distinguish between intentional error throwing (e.g. validation) and errors that may indicate bugs
export const createInstrumenter = (introspectionContext: IntrospectionContext, shatterproofModuleOverride?: string) => {
    return (ctx: ts.TransformationContext) => (sourceFile: SourceFile): SourceFile => {
        var _uselessSourceMap = new SourceMapGenerator({
            file: sourceFile.fileName,
        });

        const factory = ctx.factory;

        const executeAlias = factory.createUniqueName("shatterproof_execute").text;
        const recordLineAlias = factory.createUniqueName("shatterproof_recordLine").text;

        const findExportedFunctionsVisitor = (node: Node): Node => {
            //  declared functions only to start
            if (ts.isFunctionDeclaration(node) && node.name) {
                if (node.modifiers && node.modifiers.find(modifier => modifier.kind === ts.SyntaxKind.ExportKeyword)) {
                    introspectionContext.exported.add(node.name.text);
                }
                introspectionContext.functions.set(node.name.text, node);

            }
            return ts.visitEachChild(node, findExportedFunctionsVisitor, ctx);
        };

        const findLiteralsVisitor = (node: Node): Node => {
            if (ts.isStringLiteral(node)) {
                introspectionContext.strings.add(node.text);
            }
            if (ts.isNumericLiteral(node)) {
                introspectionContext.strings.add(node.text);
                const asNumber = parseFloat(node.text);
                introspectionContext.numbers.add(asNumber);
            }

            return ts.visitEachChild(node, findLiteralsVisitor, ctx);
        };

        const minimalText = (node: ts.Node, extra?: string) => `todo recorder per-line metadata: ${extra}`;

        const getAllLineNumbers = (...nodes: (ts.Node | undefined)[]): number[] => {
            const lines: number[] = [];
            for (const n of nodes) {
                if (!n) {
                    continue;
                }
                const startLine = ts.getLineAndCharacterOfPosition(sourceFile, n.pos).line;
                lines.push(startLine);

                const endLine = ts.getLineAndCharacterOfPosition(sourceFile, n.end).line;
                lines.push(endLine);
            }
            return lines;
        };

        const getLineNumbers = (node: ts.Node): number[] => {
            if (ts.isIfStatement(node)) {
                return getAllLineNumbers(node.expression);
            }
            if (ts.isForStatement(node)) {
                return getAllLineNumbers(
                    node.initializer,
                    node.incrementor,
                    node.condition);
            }
            if (ts.isForOfStatement(node) || ts.isForInStatement(node)) {
                return getAllLineNumbers(
                    node.initializer,
                    node.expression);
            }
            if (ts.isWhileStatement(node) || ts.isDoStatement(node)) {
                return getAllLineNumbers(node.expression);

            }
            if (ts.isSwitchStatement(node)) {
                return getAllLineNumbers(node.expression);
            }
            return getAllLineNumbers(node);
        };

        const createInstrumentationStatement = (forNode: ts.Node, lineNumber: number, extra: string) => {
            const instrumentation = factory.createExpressionStatement(factory.createCallExpression(
                factory.createIdentifier(recordLineAlias),
                undefined,
                [factory.createNumericLiteral(lineNumber),
                factory.createStringLiteral(minimalText(forNode, extra)),
                ]
            ));

            // const originalPosition = ts.getLineAndCharacterOfPosition(sourceFile, forNode.pos);
            // _uselessSourceMap.addMapping({
            //     generated: {
            //         //  TODO: don't know this yet!
            //         line: 0,
            //         column: 0,
            //     },
            //     source: sourceFile.fileName,
            //     original: {
            //         line: originalPosition.line,
            //         column: originalPosition.character,
            //     },
            // });

            //  THIS DOES NOT WORK to make sure the instrumentation statement is on the same line as the node and keep source maps happy
            //  TODO: do something like record(...) && originalStatement?
            ts.setEmitFlags(forNode, ts.EmitFlags.SingleLine);
            ts.setEmitFlags(instrumentation, ts.EmitFlags.SingleLine);

            return instrumentation;
        };

        //  TODO: generify this so that the type that comes in is the type that goes out to avoid casting
        const instrumentingVisitor = (node: Node): Node => {
            const instrumentStatementAsBlock = (factory: ts.NodeFactory, statement: ts.Statement) => {
                const lineNumberStart = ts.getLineAndCharacterOfPosition(sourceFile, statement.pos).line;
                const lineNumberEnd = ts.getLineAndCharacterOfPosition(sourceFile, statement.end).line;
                const lineNumbers = getLineNumbers(statement);
                const lineNumber = Math.max(...lineNumbers);
                const instrumentation = createInstrumentationStatement(statement, lineNumber, `pos = ${statement.pos}-${statement.end}, lines = ${lineNumberStart} - ${lineNumberEnd}; alternate = ${lineNumbers})}`);
                introspectionContext.instrumentedLines.add(lineNumber);

                const visited = instrumentingVisitor(statement);

                return factory.createBlock([visited as ts.Statement, instrumentation]);
            };

            //  TODO: instrument throws statements as those are sneaking branches (call throws exception vs. not)
            //  TODO: instrument catch blocks and finally blocks for the same reason
            //  TODO: instrument return statements as well
            //  TODO: generify this so that the type that comes in is the type that goes out to avoid casting
            const instrumentBlock = (factory: ts.NodeFactory, block: ts.Block) => {
                const newStatements: ts.Statement[] = [];
                block.statements.forEach((statement, i) => {
                    const lineNumberStart = ts.getLineAndCharacterOfPosition(sourceFile, statement.pos).line;
                    const lineNumberEnd = ts.getLineAndCharacterOfPosition(sourceFile, statement.end).line;
                    const lineNumbers = getLineNumbers(statement);
                    const lineNumber = Math.max(...lineNumbers);
                    const instrumentation = createInstrumentationStatement(statement, lineNumber, `pos = ${statement.pos}-${statement.end}, lines = ${lineNumberStart} - ${lineNumberEnd}; alternate = ${lineNumbers})}`);
                    introspectionContext.instrumentedLines.add(lineNumber);
                    newStatements.push(instrumentation);
                    newStatements.push(instrumentStatement(factory, statement));
                });
                return {
                    ...block,
                    statements: factory.createNodeArray(newStatements),
                };
            };

            const instrumentBlockOrStatement = (factory: ts.NodeFactory, node: ts.Statement | ts.Block) => {
                if (ts.isBlock(node)) {
                    return instrumentBlock(factory, node);
                }
                return instrumentStatementAsBlock(factory, node);
            };

            const instrumentStatement = (factory: ts.NodeFactory, statement: ts.Statement): ts.Statement => {
                if (ts.isIfStatement(statement)) {
                    const thenStatement = instrumentBlockOrStatement(factory, statement.thenStatement);
                    const elseStatement = statement.elseStatement
                        ? instrumentBlockOrStatement(factory, statement.elseStatement)
                        : undefined;

                    const newIfNode = {
                        ...statement,
                        thenStatement,
                        elseStatement,
                    };

                    return newIfNode;
                }

                if (ts.isIterationStatement(statement, false)) {
                    // const newStatement:ts.Statement = instrumentingVisitor(node.statement) as ts.Statement;
                    const newStatement: ts.Statement = instrumentBlockOrStatement(factory, statement.statement) as ts.Statement;
                    const newIterationNode: ts.IterationStatement = {
                        ...statement,
                        statement: newStatement,
                    };
                    return newIterationNode;
                }

                if (ts.isSwitchStatement(statement)) {
                    const newClauses =
                        statement.caseBlock.clauses.map(clause => {
                            const newStatements = ((): (ts.Statement | ts.Node)[] => {
                                if (clause.statements.length === 0) {
                                    //  create block with just instrumentation in it
                                    const emptyBlock = c(factory.createBlock([]), clause);
                                    return [instrumentBlockOrStatement(factory, emptyBlock)];
                                }
                                return clause.statements
                                    .map(statement => instrumentBlockOrStatement(factory, statement));
                            })() as ts.Statement[]; //  TODO: the cast is no bueno

                            const newClause: ts.CaseOrDefaultClause = {
                                ...clause,
                                statements: factory.createNodeArray(newStatements),
                            };
                            return newClause;
                        });

                    //  TODO: in some places the node is replaced, in others it's updated.
                    //  Make that either consistent or describe why each case should go one way or the other
                    const newSwitch: ts.SwitchStatement = {
                        ...statement,
                        caseBlock: {
                            ...statement.caseBlock,
                            clauses: factory.createNodeArray(newClauses),
                        },
                    };
                    return newSwitch;
                }

                if (ts.isBlock(statement)) {
                    return instrumentBlock(factory, statement);
                }

                return statement;
            };

            //  export top level functions so we can call them
            //  export top level classes so that we can instantiate them for functions that reference them
            if (ts.isFunctionDeclaration(node) || ts.isClassDeclaration(node)) {
                const newModifiers = [...node.modifiers ?? []];
                if (node.parent === node.getSourceFile()) { //  this means it's a top level function
                    //  Export the function
                    const exportModifier = newModifiers?.find(modifier => modifier.kind === ts.SyntaxKind.ExportKeyword);
                    if (!exportModifier) {
                        if (node.parent === node.getSourceFile()) { //  this means it's a top level function
                            const newExportModifier = factory.createModifier(ts.SyntaxKind.ExportKeyword);
                            newModifiers.push(newExportModifier);
                        }
                    }
                }

                if (ts.isFunctionDeclaration(node)) {
                    if (node.body) {
                        const modbod = instrumentBlock(factory, node.body);
                        const newFunction = factory.createFunctionDeclaration(
                            newModifiers,
                            node.asteriskToken,
                            node.name,
                            node.typeParameters,
                            node.parameters,
                            node.type,
                            modbod
                        );
                        return newFunction;
                    } else {
                        throw new Error(`Function ${node.name?.text} has no body`);
                    }
                } else {
                    //  do the instrumentation
                    const visiteded = ts.visitEachChild(node, instrumentingVisitor, ctx);

                    const newClass = factory.updateClassDeclaration(
                        visiteded as ts.ClassDeclaration,
                        newModifiers,
                        node.name,
                        node.typeParameters,
                        node.heritageClauses,
                        node.members
                    );
                    return newClass;
                }
            }

            if (ts.isArrowFunction(node)) {
                const modbod =
                    (() => {
                        if (ts.isBlock(node.body)) {
                            return instrumentBlock(factory, node.body);
                        }

                        const line = ts.getLineAndCharacterOfPosition(sourceFile, node.body.pos).line;

                        const instrumentationStatement = createInstrumentationStatement(node.body, line, "nah");
                        const returnStatement = factory.createReturnStatement(node.body);
                        const statements = factory.createNodeArray([
                            instrumentationStatement,
                            returnStatement,
                        ]);

                        return factory.createBlock(statements);

                    })();

                const newArrowFunction = factory.createArrowFunction(node.modifiers, node.typeParameters, node.parameters, node.type, node.equalsGreaterThanToken, modbod);
                // console.log(`new arrow function ${newArrowFunction.getText()}`);
                return newArrowFunction;
            }

            //  TODO: if it's a top-level variable statement of a function or arrow function type, export it
            if (ts.isVariableStatement(node)) {
                const newDeclarations = node.declarationList.declarations.map(d => {
                    if (!d.initializer) {
                        return d;
                    }

                    const newInitializer = instrumentingVisitor(d.initializer);
                    const newD = factory.createVariableDeclaration(d.name, d.exclamationToken, d.type, newInitializer as any);
                    return newD;
                });

                const newDeclarationList = factory.createVariableDeclarationList(newDeclarations, node.flags);
                const newVariableStatement = factory.createVariableStatement(node.modifiers, newDeclarationList);
                return newVariableStatement;
            }

            if (ts.isExpressionStatement(node)) {
                if (ts.isBinaryExpression(node.expression)) {
                    const newLeft = ts.visitEachChild(node.expression.left, instrumentingVisitor, ctx);
                    const newRight = ts.visitEachChild(node.expression.right, instrumentingVisitor, ctx);
                    const updatedExpression = factory.updateBinaryExpression(node.expression, newLeft, node.expression.operatorToken, newRight);
                    return factory.createExpressionStatement(updatedExpression);
                }

                return ts.visitEachChild(node.expression, instrumentingVisitor, ctx);
            }

            if (ts.isStatement(node)) {
                return instrumentStatement(factory, node);
            }

            const visiteded = ts.visitEachChild(node, instrumentingVisitor, ctx);
            return visiteded;
        };

        //  discover functions and add them to the context
        ts.visitNode(sourceFile, findExportedFunctionsVisitor);

        //  discover literal string and number values and add them to the context
        ts.visitNode(sourceFile, findLiteralsVisitor);

        //  BEGIN instrumenting code
        const visited = ts.visitNode(sourceFile, instrumentingVisitor);
        //  END instrumenting code

        //  BEGIN worker execution startup code
        //  create a literal object that maps function names to functions
        const assignments: ts.ShorthandPropertyAssignment[] = [];
        introspectionContext.functions.forEach((_, name) => {
            assignments.push(
                factory.createShorthandPropertyAssignment(
                    //  TODO: make the identifier deterministic and stable across runs
                    //  starting with line number and maybe becoming some AST path
                    factory.createIdentifier(name),
                    undefined
                ));
        });

        // const executionArguments = factory.createObjectLiteralExpression(
        //     assignments,
        //     true
        // );
        const executionArguments = factory.createIdentifier("module.exports");

        const invokeExecutionCall = factory.createExpressionStatement(factory.createCallExpression(
            factory.createIdentifier(executeAlias),
            undefined,
            [executionArguments]
        ));

        const moduleName = "shatterproof";
        const shatterproofModulePath = shatterproofModuleOverride ?? moduleName;
        const resourcedFile: SourceFile = {
            ...sourceFile,
            statements: factory.createNodeArray([
                createImportStatement(factory, `${shatterproofModulePath}/core`,
                    { name: "execute", alias: executeAlias },
                    { name: "recordLine", alias: recordLineAlias },
                ),
                //  retain other top level statements in the module because they may be necessary for setup and initialization
                visited as ts.Statement,
                invokeExecutionCall,
            ])
        };
        //  END worker execution startup code

        return resourcedFile;
    };
};
