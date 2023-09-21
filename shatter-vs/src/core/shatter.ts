import { createHash } from 'crypto';
import { mkdirSync, mkdtempSync, readdirSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import * as ts from 'typescript';
import { CombinatorialTestCaseSource, RetestCaseSource, Specimen, comparameters } from './generator';
import { Outcome, RunResult, Supervisor } from './supervisor';
import { IntrospectionContext, createInstrumenter } from './transform';
import cluster from 'cluster';
import serialize from 'canonicalize'
import { shrink } from './hybridize';
import { createId } from '@paralleldrive/cuid2';

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
    allResults: RunResult[]
    edgiest: RunResult[]
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
    options?: {
        shatterproofModuleOverride?: string,
        maxIterations?: number,
        maxTime?: number,
    }
) {
    // parse whole file into abstract syntax tree
    const [program, sourceFile] = parse(inputFile);
    const functionDeclarationNode = findFunctionNode(functionName, sourceFile);
    if (!functionDeclarationNode) {
        throw new Error(`Could not find function ${functionName}`);
    }
    // rewrite code of given function (or everything if lazy) to add instrumentation
    const [instrumented, executorScriptJs, introspectionContext] = writeInstrumented(sourceFile, options?.shatterproofModuleOverride);
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

    let count = 0;
    const maxIterations = options?.maxIterations ?? 200;
    const maxTime = options?.maxTime ?? 15_000;
    const startTime = Date.now();

    const allExecutedBranches = new Set<string>();
    const allExecutedLines = new Set<number>();

    const clusters: ResultCluster[] = [];
    const clustersByKey = new Map<string, ResultCluster>();
    const clustersBySpecimenId = new Map<string, ResultCluster>();
    const specimenResults = new Map<string, RunResult>();
    const specimensById = new Map<string, Specimen>();
    //  a set of canonicalized JSON serializations
    const parameterListsAttempted = new Set<string>();

    const onResult = (runResult: RunResult) => {
        // console.log(`Received result ${JSON.stringify(runResult)}`);
        // find the appropriate cluster or create it
        specimenResults.set(runResult.specimenId, runResult);
        const clusterKey = canonicalClusterKey(runResult);
        let cluster = clustersByKey.get(clusterKey);
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
                allResults: [],
                edgiest: [],
                totalTime: 0,
            };
            clusters.push(cluster);
            clustersByKey.set(clusterKey, cluster);
        }

        clustersBySpecimenId.set(runResult.specimenId, cluster);

        cluster.allResults.push(runResult);
        cluster.totalTime += runResult.duration;

        //  TODO: don't do this on every change
        sortClusters(clusters);

        //  update the caller
        onUpdate({ clusters, instrumentedLines: introspectionContext.instrumentedLines });
        //  update the source

        runResult.lines.forEach(line => allExecutedLines.add(line));
    };

    const supervisor = new Supervisor(modulePaths, executorScriptJs, onResult, 15);
    console.log(`tryna allExecutedBranches.size = ${allExecutedBranches.size
        // }, introspectionContext.knownBranches.size = ${introspectionContext._knownBranches.size
        }, introspectionContext.instrumentedLines.size = ${introspectionContext.instrumentedLines.size
        }`);



    // const generator = new CombinatorialTestCaseSource(program.getTypeChecker(), functionDeclarationNode.parameters);
    const source = new CombinatorialTestCaseSource(program.getTypeChecker(), introspectionContext.instrumentedLines, functionDeclarationNode);
    const generator = source.seed();

    //  SEED
    while (true) {
        const g = generator.next();
        if (g.done) {
            break;
        }

        const specimen = g.value;
        const serialized = serialize(specimen.parameters);
        if (serialized && !parameterListsAttempted.has(serialized)) {
            parameterListsAttempted.add(serialized);

            // execute those inputs in worker threads
            const worker = await supervisor.launchWorker(functionName, specimen.id, g.value.parameters);

            // TODO: if the function under test is a react component
            //  launch a headless browser
            //  capture a screenshot for each represented test case
            //  save it screenshot
            count++;
        }
    }

    await supervisor.drain();

    /*
        2) SHRINK /WEED
            10) for each cluster
                100) for each input parameter
                    1000) sort the parameter lists by that parameter
                    2000) pick the top and bottom
                    3000) shrink them
                    4000) execute
                    5000) if the result lands in the same cluster, keep shrinking
                    6000) if the result lands in a different cluster, sort that cluster
                        and see if it's an outermost parameter list (possibly replacing a previous one)
                    7000) repeat (GOTO (1000)) until the tops and bottoms of each cluster cannot be shrunk further and remain valid
                        OR the maximum attempt number is reached
    */

    const initiatedShrinkings = new Map<string, Specimen[]>();
    const enshrinken = async (baseParameters: any[], toShrinkParameterIndex:number, baseSpecimenId: string) => {
        initiatedShrinkings.set(baseSpecimenId, []);
        for (const shrunk of shrink(baseParameters[toShrinkParameterIndex])) {
            const parameters = [...baseParameters];
            parameters[toShrinkParameterIndex] = shrunk;

            const serialized = serialize(parameters);
            if (serialized && !parameterListsAttempted.has(serialized)) {
                parameterListsAttempted.add(serialized);
                initiatedShrinkings.get(baseSpecimenId)?.push(shrunk);

                const specimenId = createId();
                const parent = specimensById.get(baseSpecimenId)!;
                const newSpecimen: Specimen = {
                    id: specimenId,
                    type: 'reduction',
                    parameters,
                    parent,
                    sequence: count++,
                }
                specimensById.set(specimenId, newSpecimen);
                await supervisor.launchWorker(functionName, shrunk.id, parameters);
            }
        }
    }

    while (true) {
        const pendingShrinkings = new Set<string>();
        const completedShrinkings = new Map<string, RunResult>();
        for (const cluster of clustersByKey.values()) {
            for (let i = 0; i < functionDeclarationNode.parameters.length; i++) {
                cluster.allResults.sort((a, b) => {
                    const avalue = a.parameters[i];
                    const bvalue = b.parameters[i];
                    return comparameters(avalue, bvalue);
                });    

                const top = cluster.allResults[0];
                const toShrink = [top];

                if (cluster.allResults.length > 1) {
                    const bottom = cluster.allResults[cluster.allResults.length - 1];
                    toShrink.push(bottom);
                }    

                for (const ttt of toShrink) {
                    await enshrinken(ttt.parameters, i, ttt.specimenId);
                }
            }
        }
        await supervisor.drain();
        [...pendingShrinkings].forEach(specimenId => {
            const result = specimenResults.get(specimenId);
            if (!result) {
                throw new Error(`Unable to find result for specimen ${specimenId}`)
                return;
            }

            //  identify the cluster
            const clusterKey = canonicalClusterKey(result);
            const cluster = clustersByKey.get(clusterKey);

            const specimen = specimensById.get(specimenId);
            if (!specimen) {
                throw new Error(`Unable to find specimen ${specimenId}`)
            }
            if (specimen.type != 'reduction') {
                throw new Error("Impossible!");

            }
            const parent = specimensById.get(specimen.parent.id);
            if (!parent) {
                throw new Error("Inpossible");
            }

            const parentClusterKey = canonicalClusterKey(result);
            if (parentClusterKey !== clusterKey) {
                //  if it's different, stop shrinking this lineage
                //  no room for further shrinking
                return;
            }

            for (let i = 0; i < functionDeclarationNode.parameters.length; i++) {
            }
        });
    }

    if (storageBaseDirectory) {
        console.log(`Saving clusters to ${storageBaseDirectory}`);
        saveClusters(inputFile, storageBaseDirectory, functionName, clusters);
    }

    console.log(`Finished after ${count} iterations and ${Date.now() - startTime}ms with ${allExecutedLines.size}/${introspectionContext.instrumentedLines.size} lines executed; ${JSON.stringify(source.stats())}`);

    const sortNums = (a: number, b: number) => a - b;
    return {
        instrumented: Array.from(introspectionContext.instrumentedLines).sort(sortNums),
        executed: Array.from(allExecutedLines).sort(sortNums),
    };
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
            return a.allResults.length - b.allResults.length;
        }
        return preferredOutcomeOrder.findIndex((s) => s === a.outcome) - preferredOutcomeOrder.findIndex((s) => s === b.outcome);
    });

    clusters.forEach(cluster => {
        cluster.allResults.sort((a, b) =>
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
        const notableResults = cluster.allResults;
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

export function redux() {


    /*
        1) SEED
            10) generate a diverse array of inputs
                100) use known common edge cases
                200) analyze the source tree for special numbers or values and use those as seeds (basically all literals)
                300) use a sample of randomish fake values
                400) pull from those sets of primitive values to construct the composite objects
                500) execute

            */


    const checker: ts.TypeChecker = null as any;;
    const f: ts.FunctionDeclaration = null as any;

    const startValues: any[][] = [];
    const parameters: any[] = [];
    for (let i = 0; i < 20; i++) {
        for (let j = 0; j < f.parameters.length; j++) {
            const t = f.parameters[j].type;
            const currentType = t
                ? checker.getTypeAtLocation(t)
                : checker.getAnyType();

        }
    }



    const clusterMap = new Map<string, ResultCluster>();


    /*

2) SHRINK/WEED
10) for each cluster
    100) for each input parameter
        1000) sort the parameter lists by that parameter
        2000) pick the top and bottom
        3000) shrink them
        4000) execute
        5000) if the result lands in the same cluster, keep shrinking
        6000) if the result lands in a different cluster, sort that cluster
            and see if it's an outermost parameter list (possibly replacing a previous one)
        7000) repeat (GOTO (1000)) until the tops and bottoms of each cluster cannot be shrunk further and remain valid
            OR the maximum attempt number is reached

*/





    /*
 3) COVER/BREED
     10) sort clusters by their last line executed
     20) for each pair of cluster ([i] and [i + 1]), see if there are unexecuted lines in between
     20) if there are unexecuted lines in between, generate inputs that execute those lines
         100) identify the features that are common to the before and after
         200) identify the features that are present in the after but not the before
         300) generate more
             1000) mutate the before in ways different from (2)
             2000) hybridize the before and after
             3000) generate new inputs using different features from before
         500) execute
         600) repeat until there are no unexecuted lines OR the maximum attempt number is reached

     */
    /*

4) EDGE DETECTION/KNEAD
    10) sort clusters by their last line executed
    20) for each pair of cluster ([i] and [i + 1]), sort the input lists by each parameter position
    30) if the top of the before is distance > N from the bottom of the after, hybridize
    40) execute
    50) repeat until the maximum attempt number is reached
        OR all clusters are within the minimum distance of each other 
        OR diminishing returns on distance reduction

*/
}