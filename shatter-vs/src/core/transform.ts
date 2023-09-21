import * as ts from 'typescript';
import { Node, SourceFile } from 'typescript';

import { createId } from '@paralleldrive/cuid2';
import { FunctionDeclaration } from 'typescript';

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

export const findFunctions = (sourceFileName: string): FunctionDeclaration[] => {
    const functions: FunctionDeclaration[] = [];
    const exportedFunctions = new Set<string>();

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
            functions.push(node);

            //  do not recurse into functions; we only care about top level
        }
        return node;
    };

    const program = ts.createProgram([sourceFileName], {});
    const sourceFile = program.getSourceFile(sourceFileName);
    ts.visitNode(sourceFile, findFunctionsVisitor);
    return functions;
};

//  TODO: replace all of this with something off the shelf e.g. Istanbul or Babel
//  https://github.com/istanbuljs/istanbuljs/tree/master/packages/istanbul-lib-instrument
//  TODO: instrument every line because every line could throw an exception and thus be a branch
export const createInstrumenter = (introspectionContext: IntrospectionContext, shatterproofModuleOverride?: string) => {
    return (ctx: ts.TransformationContext) => (sourceFile: SourceFile): SourceFile => {
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
        }

        const minimalText = (node: ts.Node) => "todo recorder per-line metadata";

        const createInstrumentationStatement = (statement: ts.Statement, lineNumber: number) => {
            const instrumentation = factory.createExpressionStatement(factory.createCallExpression(
                factory.createIdentifier(recordLineAlias),
                undefined,
                [factory.createNumericLiteral(lineNumber),
                factory.createStringLiteral(minimalText(statement)),
                ]
            ));
            return instrumentation;
        };

        //  TODO: generify this so that the type that comes in is the type that goes out to avoid casting
        const instrumentingVisitor = (node: Node): Node => {
            const instrumentStatementAsBlock = (factory: ts.NodeFactory, statement: ts.Statement) => {
                const lineNumber = ts.getLineAndCharacterOfPosition(sourceFile, statement.pos).line;
                const instrumentation = createInstrumentationStatement(statement, lineNumber);
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
                    const lineNumber = ts.getLineAndCharacterOfPosition(sourceFile, statement.pos).line;
                    introspectionContext.instrumentedLines.add(lineNumber);
                    const instrumentation = createInstrumentationStatement(statement, lineNumber);
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

            //  export all functions - TODO: this may create conflicts.  A cheat would be to add an exported magic function that just does dispatch
            //  Currently DOES NOT WORK, but is it even necessary?  seems unnecessary in the extension.test.ts
            if (ts.isFunctionDeclaration(node)) {
                const exportModifier = node.modifiers?.find(modifier => modifier.kind === ts.SyntaxKind.ExportKeyword);
                if (!exportModifier) {
                    if (node.parent === node.getSourceFile()) { //  this means it's a top level function
                        const modifiers = [...node.modifiers ?? []];
                        const newExportModifier = factory.createModifier(ts.SyntaxKind.ExportKeyword);
                        // modifiers.push(newExportModifier);

                        if (node.body) {
                            const modbod = instrumentBlock(factory, node.body);
                            const newFunction = factory.createFunctionDeclaration(
                                modifiers,
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
                    }
                }
            }

            if (ts.isStatement(node)) {
                return instrumentStatement(factory, node);
            }

            const visiteded = ts.visitEachChild(node, instrumentingVisitor, ctx);
            return visiteded;
        };

        //  discover functions and add them to the context
        ts.visitNode(sourceFile, findExportedFunctionsVisitor);

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

        const executionArguments = factory.createObjectLiteralExpression(
            assignments,
            true
        );

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
