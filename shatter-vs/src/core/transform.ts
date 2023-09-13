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

const instrumentationCallNodeStacked = (factory: ts.NodeFactory, id: string, recordFunctionName: string,
    stopRecordingFunctionName: string, next: ts.NodeArray<ts.Statement>, meta: { line?: number, character?: number, filename?: string }) => {

    const metaObject = c(factory.createObjectLiteralExpression(
        [
            factory.createPropertyAssignment(
                factory.createIdentifier("line"),
                factory.createNumericLiteral(meta.line ?? -1)
            ),
            factory.createPropertyAssignment(
                factory.createIdentifier("character"),
                factory.createNumericLiteral(meta.character ?? -1)
            ),
            factory.createPropertyAssignment(
                factory.createIdentifier("filename"),
                factory.createStringLiteral(meta.filename ?? "")
            )
        ],
        true
    ), null);

    const startBlock = c(factory.createBlock(
        [factory.createExpressionStatement(factory.createCallExpression(
            factory.createIdentifier(recordFunctionName),
            undefined,
            [factory.createStringLiteral(id), metaObject]
        )), ...next],
        true
    ), null);

    const stopBlock = c(factory.createBlock(
        [factory.createExpressionStatement(factory.createCallExpression(
            factory.createIdentifier(stopRecordingFunctionName),
            undefined,
            [factory.createStringLiteral(id)]
        ))],
        true
    ), null);

    return c(factory.createBlock(
        [factory.createTryStatement(
            startBlock,
            undefined,
            stopBlock
        )],
        true
    ), null);
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

export interface Branch {
    id: string;
    originalNode: ts.Node;
    line: number;
}

export type IntrospectionContext = {
    exported: Set<string>,
    functions: Map<string, FunctionDeclaration>,
    knownBranches: Map<string, Branch>,
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
export const instrumentModule = (introspectionContext: IntrospectionContext, shatterproofModuleOverride?: string) => {
    return (ctx: ts.TransformationContext) => (sourceFile: SourceFile): SourceFile => {
        const factory = ctx.factory;

        const startRecordingAlias = factory.createUniqueName("shatterproof_startRecording").text;
        const stopRecordingAlias = factory.createUniqueName("shatterproof_stopRecording").text;
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
            const instrumentBlockOrStatement = (factory: ts.NodeFactory, instrumentationContext: IntrospectionContext, node: Node) => {
                if (!ts.isBlock(node) && !ts.isStatement(node)) {
                    const nodeKind = ts.SyntaxKind[node.kind];
                    console.log(`unexpectedly a ${nodeKind}; doing nothing`);
                    return node;
                }

                const preblock = ts.isBlock(node)
                    ? node
                    : factory.createBlock([node]);

                const block = ts.visitEachChild(preblock, instrumentingVisitor, ctx);

                const branch = createBranch(sourceFile, node, instrumentationContext);

                const meta = extractMetadata(node);
                const newStatements = instrumentationCallNodeStacked(factory, branch.id, startRecordingAlias, stopRecordingAlias, block.statements, meta);
                // const newStatements = [
                //     instrumentationCallNode(factory, id, recordAlias),
                //     ...block.statements,
                // ];

                const modded = {
                    ...block,
                    statements: [newStatements],
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

            if (ts.isBlock(node)) {
                const convertToBlockWithExplicitElses = (statements: ts.NodeArray<ts.Statement>): ts.Block => {

                    //  copy all statements until reaching an if without an else
                    //  then wrap all the rest of the block in an explicit else
                    const newStatements: ts.Statement[] = [];
                    for (let i = 0; (i < statements.length); i++) {
                        const s = statements[i];
                        //  TODO: switches and loops
                        if (!ts.isIfStatement(s) || s.elseStatement) {
                            newStatements.push(s);
                        } else {
                            //  if there are more statements
                            //  wrap them all in a single block
                            if (i + 1 < statements.length) {
                                newStatements.push(s);
                                const slice: ts.Statement[] = [];
                                for (let j = i + 1; j < statements.length; j++) {
                                    slice.push(statements[j]);
                                }
                                const converted = convertToBlockWithExplicitElses(factory.createNodeArray(slice));
                                const elseBlock = c(factory.createBlock([converted]), slice[0]);

                                const newIf = c(factory.createIfStatement(s.expression, s.thenStatement, elseBlock), s);
                                newStatements.push(newIf);
                                break;
                            }
                        }
                    }
                    const newBlock = c(factory.createBlock(newStatements), node);
                    return newBlock;
                };

                const converted = convertToBlockWithExplicitElses(node.statements);
                return instrumentBlockOrStatement(factory, introspectionContext, converted);
            }

            if (ts.isIfStatement(node)) {
                const thenStatement = instrumentBlockOrStatement(factory, introspectionContext, node.thenStatement);
                const elseStatement = node.elseStatement
                    ? instrumentBlockOrStatement(factory, introspectionContext, node.elseStatement)
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
                const newStatement: ts.Statement = instrumentBlockOrStatement(factory, introspectionContext, node.statement) as ts.Statement;
                const newIterationNode: ts.IterationStatement = {
                    ...node,
                    statement: newStatement,
                };
                return newIterationNode;
            }

            if (ts.isSwitchStatement(node)) {
                const newClauses =
                    node.caseBlock.clauses.map(clause => {
                        const newStatements = ((): (ts.Statement | ts.Node)[] => {
                            if (clause.statements.length === 0) {
                                //  create block with just instrumentation in it
                                const emptyBlock = c(factory.createBlock([]), clause);
                                return [instrumentBlockOrStatement(factory, introspectionContext, emptyBlock)];
                            }
                            return clause.statements
                                .map(statement => instrumentBlockOrStatement(factory, introspectionContext, statement));
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
            return visiteded;
        };

        //  discover functions and add them to the context
        ts.visitNode(sourceFile, findExportedFunctionsVisitor);

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
        const resourcedFile = {
            ...sourceFile,
            statements: [
                createImportStatement(factory, `${shatterproofModulePath}/core`,
                    { name: "startRecording", alias: startRecordingAlias },
                    { name: "stopRecording", alias: stopRecordingAlias },
                    { name: "execute", alias: executeAlias },
                ),
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

function extractMetadata(node: ts.Node): { line: number, character: number, filename: string } {
    if (SP_ORIGINAL_KEY in node) {
        if (node[SP_ORIGINAL_KEY] === null) {
            throw new Error(`Unexpected null original node in ${ts.SyntaxKind[node.kind]}`);
        }
        return extractMetadata((node as any)[SP_ORIGINAL_KEY]);
    }
    if (node.pos && node.pos > 0 && node.getSourceFile()) {
        const { line, character } = ts.getLineAndCharacterOfPosition(node.getSourceFile(), node.pos);
        const filename = ts.isSourceFile(node)
            ? node.fileName
            : node.getSourceFile()?.fileName;
        const meta = {
            line,
            character,
            filename,
        };
        return meta;
    }
    throw new Error(`Unable to extract metadata for node ${ts.SyntaxKind[node.kind]}`);
}

//  TODO: make IntrospectionContext a class and this a method on it
function createBranch(sourceFile: ts.SourceFile, originalNode: ts.Statement, instrumentationContext: IntrospectionContext) {
    const meta = extractMetadata(originalNode);
    const id = createId();
    const branch: Branch = { id, originalNode, line: meta.line };
    instrumentationContext.knownBranches.set(id, branch);
    return branch;
}
