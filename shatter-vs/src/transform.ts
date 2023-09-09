import * as ts from 'typescript';
import { Node, SourceFile } from 'typescript';

import { createId } from '@paralleldrive/cuid2';
import { FunctionDeclaration } from 'typescript';

const instrumentationCallNode = (factory: ts.NodeFactory, id: string) => {
    return factory.createCallExpression(
        factory.createIdentifier("record"),
        undefined,
        [factory.createStringLiteral(id)]
    );
};

const createImportStatement = (factory: ts.NodeFactory, module: string, ...things: { name: string, alias?: string }[]) => {
    const imports: ts.ImportSpecifier[] = [];

    for (const { name, alias } of things) {
        imports.push(
            factory.createImportSpecifier(false,
                alias ? factory.createIdentifier(alias) : undefined,
                factory.createIdentifier(name)
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
}

//  TODO: instrument every line because every line could throw an exception and thus be a branch
export const instrumentModule = (introspectionContext: IntrospectionContext) => {

    return (ctx: ts.TransformationContext) => (sourceFile: SourceFile): SourceFile => {
        const factory = ctx.factory;

        const findExportedFunctionsVisitor = (node: Node): Node => {
            //  declared functions only to start
            if (ts.isFunctionDeclaration(node) && node.name) {
                if (node.modifiers && node.modifiers.find(modifier => modifier.kind == ts.SyntaxKind.ExportKeyword)) {
                    introspectionContext.exported.add(node.name.text);
                }
                introspectionContext.functions.set(node.name.text, node);

            }
            return ts.visitEachChild(node, findExportedFunctionsVisitor, ctx);
        };

        const instrumentingVisitor = (node: Node): Node => {
            //  TODO: instrument throws statements as those are sneaking branches (call throws exception vs. not)
            //  TODO: instrument catch blocks and finally blocks for the same reason
            //  TODO: instrument return statements as well
            const instrumentConditionalClause = (factory: ts.NodeFactory, instrumentationContext: IntrospectionContext, node: Node) => {
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

                const instrumentation = instrumentationCallNode(factory, id);

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

            if (ts.isIfStatement(node)) {
                const thenStatement = instrumentConditionalClause(factory, introspectionContext, node.thenStatement);
                const elseStatement = node.elseStatement
                    ? instrumentConditionalClause(factory, introspectionContext, node.elseStatement)
                    : undefined;

                const newIfNode = {
                    ...node,
                    thenStatement,
                    elseStatement,
                };

                return newIfNode;
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
                const instrumentation = instrumentationCallNode(factory, id);
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

        const recordAlias = factory.createUniqueName("shatterproof_record").text;
        const executeAlias = factory.createUniqueName("shatterproof_execute").text;
        const invokeExecutionCall = factory.createExpressionStatement(factory.createCallExpression(
            factory.createIdentifier(executeAlias),
            undefined,
            [executionArguments]
        ));
        
        const resourcedFile = {
            ...sourceFile,
            statements: [
                createImportStatement(factory, "shatterproof", { name: "record", alias: recordAlias }, { name: "execute", alias: executeAlias }),
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