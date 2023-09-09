import { mkdtempSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import * as ts from 'typescript';
import { ExecutionContext } from './recorder';
import { IntrospectionContext, instrumentModule as createInstrumenter } from './transform';
import { Generator } from './generator';
import { RunResult, Supervisor } from './supervisor';

export interface ResultCluster {
    key: string
    branches: Set<string>
    results: RunResult[]
}

// TODO: iterables and generators, regular expressions, promises, tagged templates, and more
export type TestArgument = {
    name: string,
    position?: number
} & ({
    parameterStructure: 'primitive'
    argumentType: 'null' | 'undefined' | 'boolean' | 'number' | 'string' | 'symbol' | 'bigint' | 'function',
    value: any
} | {
    parameterStructure: 'object'
    argumentType: 'Date',
    value: Date
} | {
    parameterStructure: 'object'
    argumentType: 'object',
    value: Record<string, TestArgument>
} | {
    parameterStructure: 'array'
    argumentType: string,
    value: Record<string, TestArgument>
} | {
    parameterStructure: 'function',
    argumentType: string,   //  TODO: how to describe function type?
    value: any
});

//  operate on the source file instead of editor objects for generality and also to avoid having to duplicate imports
//  TODO: make sure the source file is saved before running
export async function shatterAutotest(modulePaths: string[],
    inputFile: string,
    functionName: string,
    onUpdate: (clusters: ResultCluster[]) => void,
    shatterproofModuleOverride?: string
) {
    // parse whole file into abstract syntax tree
    const [program, sourceFile] = parse(inputFile);
    const functionDeclarationNode = findFunctionNode(functionName, sourceFile);
    if (!functionDeclarationNode) {
        throw new Error(`Could not find function ${functionName}`);
    }
    // rewrite code of given function (or everything if lazy) to add instrumentation
    const [instrumented, executorScriptJs, introspectionContext] = writeInstrumented(sourceFile, shatterproofModuleOverride);
    // instrospect function and generate a set of candidate inputs

    console.log(`created ${instrumented} compiled to ${executorScriptJs}`);

    let body = null;
    for (let i = 0; i < functionDeclarationNode.getChildCount(); i++) {
        const child = functionDeclarationNode.getChildAt(i);
        if (ts.isBlock(child)) {
            body = child;
            break;
        }
    }

    if (!body) {
        throw new Error(`Could not find function body`);
    }

    const generator = new Generator(program.getTypeChecker(), functionDeclarationNode.parameters);

    const parameterLists = generator.generateRandom(10);

    let allCovered = false;
    let count = 0;
    const maxIterations = 100;
    const maxTime = 10000;
    const startTime = Date.now();

    const allExecutedBranches = new Set<string>();

    const clusters: ResultCluster[] = [];
    const onCompletion = (execution: RunResult) => {
        console.log(`Received result ${JSON.stringify(execution)}`);
        // find the appropriate cluster or create it

        onUpdate(clusters);

        // if still need to run, generate and breed more test cases and repeat
    };

    const supervisor = new Supervisor(modulePaths, executorScriptJs, onCompletion, 15);
    console.log(`tryna allExecutedBranches.size = ${allExecutedBranches.size
        }, introspectionContext.knownBranches.size = ${introspectionContext.knownBranches.size
        }, parameterLists.length = ${parameterLists.length}`);
    while (allExecutedBranches.size < introspectionContext.knownBranches.size
        && parameterLists.length > 0
        && count < maxIterations
        && Date.now() - startTime < maxTime) {


        const parameterList = parameterLists.pop();
        if (!parameterList) {
            console.error("parameterList is unexpectedly undefined");
            continue;

        }

        // execute those inputs in worker threads
        const worker = await supervisor.launchWorker(functionDeclarationNode.name?.getText() ?? '', parameterList.parameters);

        // TODO: save the test cases, results, and clusters to some directory
        // TODO: if the function under test is a react component
        //  launch a headless browser
        //  capture a screenshot for each represented test case
        //  save it screenshot
        count++;
    }
}

export function parse(sourceFilePath: string): [ts.Program, ts.SourceFile] {

    if (!sourceFilePath) {
        throw new Error(`Could not find source file for function $${sourceFilePath}`);
    }
    const program = ts.createProgram([sourceFilePath], {});

    const checker = program.getTypeChecker();

    const source = program.getSourceFile(sourceFilePath);
    if (!source) {
        throw new Error(`Could not find source file ${sourceFilePath}`);
    }

    return [program, source];
}

function findFunctionNode(functionName: string, source: ts.SourceFile): ts.FunctionDeclaration | null {
    let functionNode: ts.FunctionDeclaration | null = null;
    const visitor = (node: ts.Node) => {
        if (ts.isFunctionDeclaration(node)) {
            functionNode = node;
            return node;
        }
        ts.forEachChild(node, visitor);
    };

    ts.forEachChild(source, visitor);

    return functionNode;
}

export function writeInstrumented(sourceFile: ts.SourceFile,
    shatterproofModuleOverride?: string
): [string, string, IntrospectionContext] {

    const introspectionContext: IntrospectionContext = {
        functions: new Map(),
        exported: new Set(),
        knownBranches: new Map(),
    };

    const codeTransformer = createInstrumenter(introspectionContext, shatterproofModuleOverride);
    //  TODO: pass in project's compiler options
    const transformed = ts.transform(sourceFile, [codeTransformer]);

    const tempdir = mkdtempSync(join(tmpdir(), "shatterproof-"));
    const printer = ts.createPrinter({ newLine: ts.NewLineKind.LineFeed });

    const modifiedSourcefilePath = join(tempdir, 'temp.ts');

    const transformedSource = printer.printNode(ts.EmitHint.Unspecified, transformed.transformed[0], transformed.transformed[0]);

    writeFileSync(modifiedSourcefilePath, transformedSource);

    const modifiedProgram = ts.createProgram([modifiedSourcefilePath], {});
    const modifiedSource = modifiedProgram.getSourceFile(modifiedSourcefilePath);
    if (!modifiedSource) {
        throw new Error(`Could not find source file ${modifiedSourcefilePath}`);
    }
    //  TODO: how to know what the filename is?  Is that what writeFileCallback does?
    //  Or does that replace the file writing that would otherwise happen?
    modifiedProgram.emit();
    const executorScriptJs = modifiedSourcefilePath.replace(/\.tsx?$/, '.js');

    //  write a new version of the function with instrumentation
    //  replace it in the AST


    return [modifiedSourcefilePath, executorScriptJs, introspectionContext];

}

export function generateArgumentList(seed: number) {

}

