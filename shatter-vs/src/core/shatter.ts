import { createHash } from 'crypto';
import { mkdirSync, mkdtempSync, readdirSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import * as ts from 'typescript';
import { CCombinatorialTestCaseSource, RetestCaseSource } from './generator';
import { Outcome, RunResult, Supervisor } from './supervisor';
import { IntrospectionContext, createInstrumenter } from './transform';

export interface AutotestResults {
    clusters: ResultCluster[];
    instrumentedLines: Set<number>;
}

//  TODO: for error cases add the file and line of where it was thrown and also
//  the file and line of the first line in the instrumented code
export interface ResultCluster {
    key: string
    lines: number[]
    //  includes potential duplicates if the same line is hit twice
    linesInOrder: number[]
    results: RunResult[]
    outcome: Outcome
    totalTime: number
}

function sha1(value: string, options?: { salt?: string, maxLength?: number }): string {
    const shasum = createHash('sha1');
    shasum.update(value);
    if (options?.salt) {
        shasum.update(options.salt);
    }
    const hexed = shasum.digest('hex');
    return hexed.substring(0, options?.maxLength ?? 40);
}

function canonicalClusterKey(result: RunResult) {
    const smashed = {
        lines: Array.from(result.lines).sort(),
        completed: result.completed,
        error: !!result.error,
    };

    const shasum = createHash('sha1');
    shasum.update(JSON.stringify(smashed));
    //  distinguish by return condition as well as branches taken
    const key = shasum.digest('hex');
    
    console.log(`key ${key} => ${JSON.stringify(smashed)}`);
    return key;
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

export async function shatterRetest(modulePaths: string[],
    inputFile: string,
    storageBaseDirectory: string,
    functionName: string,
    onUpdate: (results: AutotestResults) => void,
    shatterproofModuleOverride?: string) {

        const inputFileHash = sha1(inputFile);
        const clusterStorageDirectory = join(storageBaseDirectory, 'clusters', inputFileHash, functionName);

        //  list all files
        const clusters: ResultCluster[] = [];
        readdirSync(clusterStorageDirectory).forEach(clusterFile => {
            const cluster = JSON.parse(clusterFile);
            clusters.push(cluster);
        });

        const generator = new RetestCaseSource(clusters);
    
    }

//  operate on the source file instead of editor objects for generality and also to avoid having to duplicate imports
//  TODO: make sure the source file is saved before running
//  TODO: collapse the abstract syntax tree into a tree of conditions and blocks
export async function shatterAutotest(modulePaths: string[],
    inputFile: string,
    storageBaseDirectory: string | undefined,
    functionName: string,
    onUpdate: (results: AutotestResults) => void,
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

    console.log(`created ${instrumented} compiled to ${executorScriptJs} with storageBaseDirectory = ${storageBaseDirectory}`);

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

    const generator = new CCombinatorialTestCaseSource(program.getTypeChecker(), functionDeclarationNode.parameters);

    const parameterLists = generator.generateRandom(10);

    let count = 0;
    const maxIterations = 100;
    const maxTime = 10000;
    const startTime = Date.now();

    const allExecutedBranches = new Set<string>();
    const allExecutedLines = new Set<number>();

    const clusters: ResultCluster[] = [];
    const clusterMap = new Map<string, ResultCluster>();

    const onResult = (runResult: RunResult) => {
        // console.log(`Received result ${JSON.stringify(runResult)}`);
        // find the appropriate cluster or create it

        updateClusters(runResult, clusterMap, clusters);

        onUpdate({ clusters, instrumentedLines: introspectionContext.instrumentedLines });

        runResult.lines.forEach(line => allExecutedLines.add(line));
        // if still need to run, generate and breed more test cases and repeat
        if (allExecutedLines.size < introspectionContext.instrumentedLines.size) {
            const unreachedInstrumentedLines: number[] = [];
            for (const instrumentedLine of introspectionContext.instrumentedLines) {
                if (!allExecutedLines.has(instrumentedLine)) {
                    unreachedInstrumentedLines.push(instrumentedLine);
                }
            }

            const firstUnreachedLine = unreachedInstrumentedLines[0];
            //  TODO: smart things
            /*
              1) map inputs to lines (have that from RunResult[])
              2) look at the input that make it to that point and those that don't make it to that point; the former have something necessary
              3) look at the inputs that make it past that point; those LACK something necessary
              4) generate mutations of the ones from (2)
            */

            clusters.forEach(cluster => {
                // cluster.branches.includes(firstUnreached.getText())
            });
        }
    };

    const supervisor = new Supervisor(modulePaths, executorScriptJs, onResult, 15);
    console.log(`tryna allExecutedBranches.size = ${allExecutedBranches.size
        // }, introspectionContext.knownBranches.size = ${introspectionContext._knownBranches.size
        }, introspectionContext.instrumentedLines.size = ${introspectionContext.instrumentedLines.size
        }, parameterLists.length = ${parameterLists.length}`);
    while (allExecutedLines.size < introspectionContext.instrumentedLines.size
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

        // TODO: if the function under test is a react component
        //  launch a headless browser
        //  capture a screenshot for each represented test case
        //  save it screenshot
        count++;
    }

    await supervisor.drain();

    if (storageBaseDirectory) {
        console.log(`Saving clusters to ${storageBaseDirectory}`);
        saveClusters(inputFile, storageBaseDirectory, functionName, clusters);
    }
    console.log(`Finished after ${count} iterations`);

    const sortNums = (a: number, b: number) => a - b;
    return {
        instrumented: Array.from(introspectionContext.instrumentedLines).sort(sortNums),
        executed: Array.from(allExecutedLines).sort(sortNums),
    };
}

function updateClusters(runResult: RunResult, clusterMap: Map<string, ResultCluster>, clusters: ResultCluster[]) {
    const clusterKey = canonicalClusterKey(runResult);
    let cluster = clusterMap.get(clusterKey);
    if (!cluster) {
        const outcome = ((): Outcome => {
            if (runResult.completed) {
                if (runResult.error) {
                    return 'error';
                }
                return 'completed';
            }
            return 'timeout';
        })();

        cluster = {
            key: clusterKey,
            lines: runResult.lines,
            linesInOrder: runResult.linesInOrder,
            outcome,
            results: [],
            totalTime: 0,
        };
        clusters.push(cluster);
        clusterMap.set(clusterKey, cluster);
    }

    cluster.results.push(runResult);
    cluster.totalTime += runResult.duration;

    //  TODO: don't do this on every change
    sortClusters(clusters);
}

function sortClusters(clusters: ResultCluster[]) {
    const preferredOutcomeOrder: Outcome[] = ['failed', 'error', 'timeout', 'completed'];
    clusters.sort((a, b) => {
        if (a.outcome === b.outcome) {
            for (let i = 0; i < a.lines.length && i < b.lines.length; i++) {
                if (a.lines[i] !== b.lines[i]) {
                    return a.lines[i] - b.lines[i];
                }
            }
            return a.results.length - b.results.length;
        }
        return preferredOutcomeOrder.findIndex((s) => s === a.outcome) - preferredOutcomeOrder.findIndex((s) => s === b.outcome);
    });

    clusters.forEach(cluster => {
        cluster.results.sort((a, b) =>
            JSON.stringify(a.parameters).localeCompare(JSON.stringify(b.parameters))
        );
    });
}

function saveClusters(inputFile: string, storageBaseDirectory: string, functionName: string, clusters: ResultCluster[]) {
    const inputFileHash = sha1(inputFile);
    const clusterStorageDirectory = join(storageBaseDirectory, 'clusters', inputFileHash, functionName);

    mkdirSync(clusterStorageDirectory, { recursive: true });

    //  save every cluster
    for (const cluster of clusters) {
        const clusterStorageFile = join(clusterStorageDirectory, `${cluster.key}.json`);

        //  TODO: actually filter in some meaningful way
        const notableResults = cluster.results;
        const clusterToWrite = {
            ...cluster,
            results: notableResults,
        };

        //  TODO: merge with what exists
        //  TODO: avoid filling the drive
        writeFileSync(clusterStorageFile, JSON.stringify(cluster, null, 2));
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
        if (ts.isFunctionDeclaration(node) && node.name?.getText() === functionName) {
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
        instrumentedLines: new Set(),
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