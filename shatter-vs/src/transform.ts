import * as ts from 'typescript';
import { Node, SourceFile } from 'typescript';

import { createId } from '@paralleldrive/cuid2';
import { FunctionDeclaration } from 'typescript';
import { join } from 'path';

const instrumentationCallNode = (factory: ts.NodeFactory, id: string, recordFunctionName: string) => {
    return factory.createCallExpression(
        factory.createIdentifier(recordFunctionName),
        undefined,
        [factory.createStringLiteral(id)]
    );
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
    knownBranches: Map<string, Node>,
};

const hasEarlyReturn = (node: ts.Statement | ts.Block): boolean => {
    if (ts.isBlock(node)) {
        !!node.statements.find(hasEarlyReturn);
    }
    return ts.isReturnStatement(node) || ts.isThrowStatement(node);
};

//  TODO: instrument every line because every line could throw an exception and thus be a branch
export const instrumentModule = (introspectionContext: IntrospectionContext, shatterproofModuleOverride?:string) => {

    return (ctx: ts.TransformationContext) => (sourceFile: SourceFile): SourceFile => {
        const factory = ctx.factory;
        const recordAlias = factory.createUniqueName("shatterproof_record").text;
        const executeAlias = factory.createUniqueName("shatterproof_execute").text;

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

        //  TODO: generify this so that the type that comes in is the type that goes out to avoid casting
        const instrumentingVisitor = (node: Node): Node => {
            //  TODO: instrument throws statements as those are sneaking branches (call throws exception vs. not)
            //  TODO: instrument catch blocks and finally blocks for the same reason
            //  TODO: instrument return statements as well
            //  TODO: generify this so that the type that comes in is the type that goes out to avoid casting
            const instrumentClause = (factory: ts.NodeFactory, instrumentationContext: IntrospectionContext, node: Node) => {
                if (!ts.isBlock(node) && !ts.isStatement(node)) {
                    const nodeKind = ts.SyntaxKind[node.kind];
                    console.log(`unexpectedly a ${nodeKind}; doing nothing`);
                    return node;
                }

                const preblock = ts.isBlock(node)
                    ? node
                    : factory.createBlock([node]);

                const block = ts.visitEachChild(preblock, instrumentingVisitor, ctx);

                const id = createId();
                instrumentationContext.knownBranches.set(id, node);

                const instrumentation = instrumentationCallNode(factory, id, recordAlias);

                const modded = {
                    ...block,
                    statements:
                        [
                            instrumentation,
                            ...block.statements,
                        ]
                };

                return modded;
            };

            //  export all functions - TODO: this may create conflicts.  A cheat would be to add an exported magic function that just does dispatch
            //  Currently DOES NOT WORK, but is it even necessary?  seems unnecessary in the extension.test.ts
            if (ts.isFunctionDeclaration(node)) {
                const exportModifier = node.modifiers?.find(modifier => modifier.kind === ts.SyntaxKind.ExportKeyword);
                if (!exportModifier) {
                    if (node.parent === node.getSourceFile()) { //  this means it's a top level function
                        const modifiers = [...node.modifiers ?? []];
                        const newExportModifier = factory.createModifier(ts.SyntaxKind.ExportKeyword);
                        modifiers.push(newExportModifier);
                        factory.updateFunctionDeclaration(
                            node,
                            modifiers,
                            node.asteriskToken,
                            node.name,
                            node.typeParameters,
                            node.parameters,
                            node.type,
                            node.body
                        );
                    }
                }
            }

            if (ts.isIfStatement(node)) {
                const thenStatement = instrumentClause(factory, introspectionContext, node.thenStatement);
                const elseStatement = node.elseStatement
                    ? instrumentClause(factory, introspectionContext, node.elseStatement)
                    : undefined;

                const newIfNode = {
                    ...node,
                    thenStatement,
                    elseStatement,
                };

                return newIfNode;
            }

            if (ts.isIterationStatement(node, false)) {
                // const newStatement:ts.Statement = instrumentingVisitor(node.statement) as ts.Statement;
                const newStatement: ts.Statement = instrumentClause(factory, introspectionContext, node.statement) as ts.Statement;
                const newIterationNode: ts.IterationStatement = {
                    ...node,
                    statement: newStatement,
                };
                return newIterationNode;
            }

            if (ts.isSwitchStatement(node)) {
                const newClauses =
                    node.caseBlock.clauses.map(clause => {
                        const newStatements = (():(ts.Statement|ts.Node)[] => {
                            if (clause.statements.length === 0) {
                                //  create block with just instrumentation in it
                                return [instrumentClause(factory, introspectionContext, factory.createBlock([]))];
                            }
                            return clause.statements
                            .map(statement => instrumentClause(factory, introspectionContext, statement));
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
                    ...node,
                    caseBlock: {
                        ...node.caseBlock,
                        clauses: factory.createNodeArray(newClauses),
                    },
                };
                return newSwitch;
            }

            const visiteded = ts.visitEachChild(node, instrumentingVisitor, ctx);

            if (!ts.isBlock(visiteded)) {
                return visiteded;
            }

            //  special case to find and instrument implicit else clauses
            //  this does not match the logic that explicitly instruments then or else clauses
            //  nor is the instrumentation at the top of the block sufficient
            const newStatements: (ts.Statement | ts.CallExpression)[] = [];
            let referenceStatementIdForInstrumentation: string | null = null;
            //  look for if statements without else clauses
            //  if there's a return or throw in the then clause, instrument the next statement after the if
            visiteded.statements.forEach((statement, index) => {
                //  always duplicate the current statement
                newStatements.push(statement);

                if (referenceStatementIdForInstrumentation) {
                    introspectionContext.knownBranches.set(referenceStatementIdForInstrumentation, statement); //  the statement before the implicit else; actually want the statement AFTER
                    referenceStatementIdForInstrumentation = null;
                }
                if (!ts.isIfStatement(statement)) {
                    return;
                }
                if (statement.elseStatement) {
                    return;
                }
                if (!hasEarlyReturn(statement.thenStatement)) {
                    return;
                }
                if (ts.isReturnStatement(statement.thenStatement) || ts.isThrowStatement(statement.thenStatement)) {
                    return;
                }
                if (!ts.isBlock(statement.thenStatement)) {
                    return;
                }

                const isExitingStatement = statement.thenStatement.statements.find((thenBlockStatement, index) => ts.isReturnStatement(thenBlockStatement) || ts.isThrowStatement(thenBlockStatement));
                if (!isExitingStatement) {
                    return;
                }

                const id = createId();
                const instrumentation = instrumentationCallNode(factory, id, recordAlias);
                newStatements.push(instrumentation);
                referenceStatementIdForInstrumentation = id;
            });

            const modded = {
                ...visiteded,
                statements: newStatements
            };

            return modded;
        };

        //  discover functions and add them to the context
        ts.visitNode(sourceFile, findExportedFunctionsVisitor);

        //  BEGIN worker execution startup code
        //  create a literal object that maps function names to functions
        const assignments: ts.ShorthandPropertyAssignment[] = [];
        introspectionContext.functions.forEach((_, name) => {
            assignments.push(
                factory.createShorthandPropertyAssignment(
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
        const resourcedFile = {
            ...sourceFile,
            statements: [
                createImportStatement(factory, shatterproofModulePath, { name: "record", alias: recordAlias }, { name: "execute", alias: executeAlias }),
                //  retain other top level statements in the module because they may be necessary for setup and initialization
                ...sourceFile.statements,
                invokeExecutionCall,
            ]
        };
        //  END worker execution startup code
        
        //  BEGIN instrumenting code -- TODO: why is this after the worker execution code?
        const visited = ts.visitNode(resourcedFile, instrumentingVisitor);
        //  END instrumenting code

        return visited.getSourceFile();
    };
};