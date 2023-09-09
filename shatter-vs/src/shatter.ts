import { mkdtempSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import * as ts from 'typescript';
import { ExecutionContext } from './recorder';
import { IntrospectionContext, instrumentModule } from './transform';
import { Generator } from './generator';
import { Supervisor } from './supervisor';

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

export interface Execution {
    args: any[],
    result: any,
    error: any
    duration?: number
}

//  operate on the source file instead of editor objects for generality and also to avoid having to duplicate imports
//  TODO: make sure the source file is saved before running
export function shatterAutotest(modulePaths:string[], functionNode: ts.FunctionDeclaration) {
    // parse whole file into abstract syntax tree
    const [program, ast] = parse(functionNode);
    // rewrite code of given function (or everything if lazy) to add instrumentation
    const [instrumented, executorScriptJs, introspectionContext] = writeInstrumented(functionNode);
    // instrospect function and generate a set of candidate inputs

    const generator = new Generator(program.getTypeChecker(), functionNode.parameters);

    const parameterLists = generator.generateRandom(10);
    
    let allCovered = false;
    let count = 0;
    const maxIterations = 100;
    const maxTime = 10000;
    const startTime = Date.now();

    const executionContext:ExecutionContext = {
        executedBranches: new Set(),
    };

    const supervisor = new Supervisor(modulePaths, introspectionContext, executorScriptJs, 15)

    while (executionContext.executedBranches.size < introspectionContext.knownBranches.size
         && parameterLists.length > 0
         && count < maxIterations
         && Date.now() - startTime < maxTime) {


        // execute those inputs in worker threads
        // as each thread finishes, find the appropriate cluster or create it
        // after creating a new cluster or adding a qualified test case to a cluster, update the tree, showing up to N (~20) clusters with up to M (~10) test cases each, prioritizing the edge cases
        // if still need to run, generate and breed more test cases and repeat
        // if the function under test is a react component, launch a headless browser and capture a screenshot for each represented test case
        // save the test cases, results, clusters, and screenshots to some directory

        count++;
    }
}

export function parse(functionNode: ts.FunctionDeclaration): [ts.Program, ts.SourceFile] {

    const sourceFilePath = functionNode.getSourceFile().fileName;
    const program = ts.createProgram([sourceFilePath], {});

    const checker = program.getTypeChecker();

    const source = program.getSourceFile(sourceFilePath);
    if (!source) {
        throw new Error(`Could not find source file ${sourceFilePath}`);
    }

    return [program, source];
}

export function writeInstrumented(functionNode: ts.FunctionDeclaration):[string, string, IntrospectionContext] {

    const introspectionContext:IntrospectionContext = {
        functions: new Map(),
        exported: new Set(),
        knownBranches: new Map(),
    };

    const codeTransformer = instrumentModule(introspectionContext);
    //  TODO: pass in project's compiler options
    const transformed = ts.transform(functionNode.getSourceFile(), [codeTransformer]);
    
    const tempdir = mkdtempSync(join(tmpdir(), "shatterproof-"));
    const printer = ts.createPrinter({ newLine: ts.NewLineKind.LineFeed });

    const modifiedSourcefilePath = join(tempdir, 'temp.ts');
    const executorScriptJS = join(tempdir, 'temp.js');

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

    if (functionNode.getChildCount() !== 1) {
        throw new Error(`Expected function node to have exactly one child, but found ${functionNode.getChildCount()}`);
    }

    let body = null;
    for (let i = 0; i < functionNode.getChildCount(); i++) {
        const child = functionNode.getChildAt(i);
        if (ts.isBlock(child)) {
            body = child;
            break;
        }
    }

    if (!body) {
        throw new Error(`Could not find function body`);
    }


    return [modifiedSourcefilePath, executorScriptJs, introspectionContext];

}

export function generateArgumentList(seed: number) {

}

