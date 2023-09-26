import { diff, addedDiff, deletedDiff, updatedDiff, detailedDiff } from 'deep-object-diff';
import { createId } from '@paralleldrive/cuid2';
import { createHash } from 'crypto';
import { mkdirSync, mkdtempSync, readdirSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import * as ts from 'typescript';
import { BaseSpecimen, CombinatorialTestCaseSource, RetestCaseSource, Specimen } from './generator';
import { hybridize, isStrictExtension, shrink } from './hybridize';
import { Outcome, RunResult, Supervisor } from './supervisor';
import { IntrospectionContext, createInstrumenter } from './transform';
import { isEqual } from 'lodash';
import cluster from 'cluster';
import { canonicallyStringify, comparameters, computeDistance } from './util';
import { Invocation } from './worker-protocol';

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
    //  one to one with parameter list
    leasts: Specimen[]
    //  one to one with parameter list
    mosts: Specimen[]
    outcome: Outcome
    totalTime: number
    distancesToClusters: Map<string, number>[]
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

    // console.log(`key ${key} => ${JSON.stringify(smashed)}`);
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
    const [instrumentedFile, executorScriptJs, introspectionContext] = writeInstrumented(sourceFile, options?.shatterproofModuleOverride);
    const functionStartLine = ts.getLineAndCharacterOfPosition(sourceFile, functionDeclarationNode.pos).line;
    const functionEndLine = ts.getLineAndCharacterOfPosition(sourceFile, functionDeclarationNode.end).line;

    const instrumentedFunctionLines = new Set<number>();
    for (let functionLine = functionStartLine; functionLine < functionEndLine; functionLine++) {
        if (introspectionContext.instrumentedLines.has(functionLine)) {
            instrumentedFunctionLines.add(functionLine);
        }
    }

    console.log(`created ${instrumentedFile} compiled to ${executorScriptJs} with storageBaseDirectory = ${storageBaseDirectory}`);

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
        const specimen = specimensById.get(runResult.specimenId)!;
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
                results: [],
                //  the single specimen has the values that are the least and the most for all parameters
                leasts: runResult.parameters.map(_ => specimen),
                mosts: runResult.parameters.map(_ => specimen),
                totalTime: 0,
                distancesToClusters: runResult.parameters.map(_ => new Map()),
            };
            clusters.push(cluster);
            clustersByKey.set(clusterKey, cluster);
        }

        clustersBySpecimenId.set(runResult.specimenId, cluster);

        cluster.results.push(runResult);
        cluster.totalTime += runResult.duration;

        //  TODO: don't do this on every change
        sortClusters(clusters);

        //  update the caller
        onUpdate({ clusters, instrumentedLines: introspectionContext.instrumentedLines });
        //  update the source

        runResult.lines.forEach(line => allExecutedLines.add(line));
    };

    const supervisor = new Supervisor(modulePaths, executorScriptJs, 15);
    console.log(`tryna allExecutedBranches.size = ${allExecutedBranches.size
        // }, introspectionContext.knownBranches.size = ${introspectionContext._knownBranches.size
        }, introspectionContext.instrumentedLines.size = ${introspectionContext.instrumentedLines.size
        }`);


    /*

    1) determine the absolute minimal acceptable parameter list
    2) run that
    3) mutate the parameter list
    4) elaborate on the parameter list
    5) if one of them goes somewhere new, bisect
    6) go to the first hole; goto 3


    */

    // const generator = new CombinatorialTestCaseSource(program.getTypeChecker(), functionDeclarationNode.parameters);
    const source = new CombinatorialTestCaseSource(program.getTypeChecker(), introspectionContext.instrumentedLines, functionDeclarationNode);
    const generator = source.seed({numbers:introspectionContext.numbers, strings:introspectionContext.strings});

    async function evaluateSpecimen(basimen: BaseSpecimen) {
        const serialized = canonicallyStringify(basimen.parameters);
        if (serialized && !parameterListsAttempted.has(serialized)) {
            parameterListsAttempted.add(serialized);

            const specimenId = createId();

            const newSpecimen: Specimen = {
                id: specimenId,
                sequence: count++,
                ...basimen,
            };
            specimensById.set(specimenId, newSpecimen);
            // console.log(`Evaluating specimen ${JSON.stringify(newSpecimen)}`);
            await supervisor.launchWorker(functionName, specimenId, newSpecimen.parameters, (invocation: Invocation, result: RunResult) => {
                onResult(result);
            });
            return specimenId;
        }
    }

    //  SEED
    const maxSeeds = 10;
    const maxShrinkGenerations = 10;
    const minResultsPerCluster = 4;

    while (count < maxIterations && Date.now() - startTime < maxTime) {
        const toSeed = Math.max(introspectionContext.instrumentedLines.size - allExecutedLines.size, 5) * maxSeeds;
        await seed(toSeed, generator, evaluateSpecimen);
        await supervisor.drain();

        //  WEED
        await weed(evaluateSpecimen, maxShrinkGenerations, clustersByKey, functionDeclarationNode.parameters, specimensById, supervisor);
        await supervisor.drain();

        //  BREED
        await breed(evaluateSpecimen, introspectionContext.instrumentedLines, allExecutedLines, clusters);
        await supervisor.drain();

        //  KNEAD
        //  only do clusters that have distance > 1 from neighbors
        //  and if they've gotten closer recently
        await knead(evaluateSpecimen, clustersByKey, functionDeclarationNode.parameters);
        await supervisor.drain();

        if (introspectionContext.instrumentedLines.size - allExecutedLines.size === 0) {
            //  TODO: continue until at least N examples in each cluster
            const incompleteClusters = clusters.filter(c => c.results.length < minResultsPerCluster);
            if (incompleteClusters.length === 0) {
                break;
            }
        }
    }

    await supervisor.terminate();

    const executed = Array.from(allExecutedLines).sort((a, b) => a - b);
    const instrumented = Array.from(introspectionContext.instrumentedLines).sort((a, b) => a - b);

    console.log(`Finished after ${count} iterations and ${Date.now() - startTime}ms with ${allExecutedLines.size}/${introspectionContext.instrumentedLines.size} lines executed`);

    return { executed, instrumented, clusters };
}


async function seed(maxSeeds: number, generator: Iterator<Specimen, any, undefined>, evaluateSpecimen: (basimen: BaseSpecimen) => Promise<string | undefined>) {
    for (let i = 0; i < maxSeeds; i++) {
        const g = generator.next();
        if (g.done) {
            break;
        }

        await evaluateSpecimen(g.value);
    }
}

const findFirstHole = (c: ResultCluster, instrumentedLines: number[]) => {
    for (const lineNumber of instrumentedLines) {
        if (!c.lines.includes(lineNumber)) {
            return lineNumber;
        }
    }
};


//  TODO: identify sets of jsonpath => value that are common to all specimens in all clusters that hit a particular line
/*
    foreach specimen that got to a given line, find the minimal version of that specimen that still gets to that line
*/
async function breed(evaluateSpecimen: (b: BaseSpecimen) => Promise<string | undefined>, allInstrumentedLines: Set<number>, allExecutedLines: Set<number>, _clusters: ResultCluster[]) {


    function breedForClusters(baseClusters: ResultCluster[], overshootClusters: ResultCluster[]) {
        //  identify differences between baseClusters and overshootClusters


    }

    const instrumentedLines = Array.from(allInstrumentedLines).sort((a, b) => a - b);
    const clustersOrderByFirstHolePosition = [..._clusters];
    clustersOrderByFirstHolePosition.sort((a, b) => {
        a.lines.sort((a, b) => a - b);
        const aFirstSkipped = findFirstHole(a, instrumentedLines);
        const bFirstSkipped = findFirstHole(a, instrumentedLines);
        if (aFirstSkipped === undefined) {
            if (bFirstSkipped === undefined) {
                return 0;
            }
            return -1;
        }
        if (bFirstSkipped === undefined) {
            return 1;
        }
        return aFirstSkipped - bFirstSkipped;
    });

    const clustersByLine = new Map<number, ResultCluster[]>();


    //  track which clusters got closest to a given line
    const lastBefore = new Map<number, ResultCluster[]>();
    const firstAfter = new Map<number, ResultCluster[]>();
    for (const l of instrumentedLines) {
        lastBefore.set(l, []);
        firstAfter.set(l, []);
        clustersByLine.set(l, []);
    }

    for (const c of clustersOrderByFirstHolePosition) {
        for (const l of c.lines) {
            clustersByLine.get(l)!.push(c);
        }
    }

    let lastExecutedLine = instrumentedLines[0];
    let previousLineClusters = clustersByLine.get(lastExecutedLine);
    if (previousLineClusters === undefined) {
        //  TODO: this can happen if the only clusters are failed or timed out clusters
        throw new Error(`No clusters for first instrumented line ${instrumentedLines[0]}`);
    }

    let currentGap: number[] = [];
    for (let i = 1; i < instrumentedLines.length; i++) {
        //  should always have a value; the ?? is just so Typescript won't complain, and I dislike !
        const currentLineClusters = clustersByLine.get(instrumentedLines[i]) ?? [];
        if (currentLineClusters.length > 0) {
            if (currentGap.length > 0) {
                //  just found the end of a gap
                breedForClusters(previousLineClusters, currentLineClusters);
            }

            currentGap = [];

            previousLineClusters = currentLineClusters;
            lastExecutedLine = instrumentedLines[i];
            continue;
        }

        currentGap.push(instrumentedLines[i]);
    }

    if (currentGap.length > 0) {
        //  just found the end of a gap
        breedForClusters(previousLineClusters, []);
    }

    let previous = clustersOrderByFirstHolePosition[0];
    for (let i = 1; i < clustersOrderByFirstHolePosition.length; i++) {
        const current = clustersOrderByFirstHolePosition[i];

        for (let j = 0; j < instrumentedLines.length; j++) {
            const lineNumber = instrumentedLines[j];

            if ((clustersByLine.get(lineNumber)?.length ?? 0) > 0) {
                //  some other cluster hit this line, so we don't need coverage
                continue;
            }

            const prevhas = previous.lines.includes(lineNumber);
            const currhas = current.lines.includes(lineNumber);
            //  if they behave the same, we don't care
            if (prevhas === currhas) {
                continue;
            }

            if (prevhas) {
                const missingLines: number[] = [lineNumber];
                for (let k = j; k < instrumentedLines.length; k++) {
                    const currhas2 = current.lines.includes(instrumentedLines[k]);
                    if (currhas2) {
                        break;
                    }
                    missingLines.push(instrumentedLines[k]);
                }
                const missingLinesSet = new Set(missingLines);
                for (const m of missingLines) {
                    for (let k = 0; k < clustersOrderByFirstHolePosition.length; k++) {
                        if (k === i || k === i - 1) {
                            continue;
                        }
                        for (const cline of clustersOrderByFirstHolePosition[k].lines) {
                            missingLinesSet.delete(cline);
                        }
                    }
                }
                //  some other clusters are covering these missing lines, so we don't need to look for ways to hit them
                if (missingLinesSet.size === 0) {
                    continue;
                }


            }
        }

        const previousFirstHole = findFirstHole(previous, instrumentedLines);
        const currentFirstHole = findFirstHole(current, instrumentedLines);
        if (previousFirstHole !== undefined) {
            if (currentFirstHole === undefined) {
                throw new Error(`Clusters out of order, ${previousFirstHole} should be before ${currentFirstHole}`);
            }
            if (previousFirstHole === currentFirstHole) {
                continue;
            }
        } else if (currentFirstHole !== undefined) {
            throw new Error(`Clusters out of order, ${previousFirstHole} should be before ${currentFirstHole}`);
        }


        const previousLast = previous.lines[previous.lines.length - 1];
        const currentFirst = current.lines[0];
        if (currentFirst !== previousLast + 1) {
            // throw new Error(`Clusters out of order, ${previousLast} should be before ${currentFirst}`);
        }
        previous = current;
    }

    /**
     * mutation
     * 1) find lines that have been instrumented but not executed
     * 2) identify clusters that have exercised the lines before and/or after
     * 3) generate parameter lists that are similar to the ones
     *      used to get to the before and different from the after
     * 
     * 
     */

    let mutations = 0;
    const allInstrumentedLinesInOrder = Array.from(allInstrumentedLines).sort();
    let lastBeforeFirstExecuted: number | undefined = undefined;
    let firstUnexecuted: number | undefined = undefined;
    let i = 0;
    for (; i < allInstrumentedLinesInOrder.length; i++) {
        const line = allInstrumentedLinesInOrder[i];
        if (!allExecutedLines.has(line)) {
            firstUnexecuted = line;
            break;
        }
        lastBeforeFirstExecuted = line;
    }

    /*
    //  in theory a tree type structure seems like the way to go here,
    //  but (I think) simple line numbers do well enough; if we have some
    //  code that got executed, then some code that didn't, and then optionally
    //  some more code that, we can be pretty confident that the middle part was
    //  in conditional or loop body, and that what got executed later is 
    //  either an explicit else, an implicit else, or just normal unconditional
    //  execution but either way it didn't satisfy the requirements of the missing
    //  part, so we can say we want inputs like what got to the first part but unlike what got
    //  to the third part.
    */
    if (firstUnexecuted !== undefined) {
        let firstExecutedAfter: number | undefined = undefined;
        for (; i < allInstrumentedLinesInOrder.length; i++) {
            const line = allInstrumentedLinesInOrder[i];
            if (allExecutedLines.has(line)) {
                firstExecutedAfter = line;
                break;
            }
        }

        //  if at least one line was executed...
        if (lastBeforeFirstExecuted !== undefined) {
            //  otherwise Typescript doesn't know that lastBeforeFirstExecuted is defined
            const lbfe = lastBeforeFirstExecuted;
            //  should be in order from lowest last line to highest last line
            //  based on the sorting done before bisection
            const ranBefore = clustersOrderByFirstHolePosition.filter(c => c.lines.includes(lbfe));
            const ranAfter = clustersOrderByFirstHolePosition.filter(c => firstExecutedAfter && c.lines.includes(firstExecutedAfter));
            const ranBeforeOnly = ranBefore.filter(c => !ranAfter.includes(c));

            if (firstExecutedAfter === undefined) {
                //  apparently we executed nothing from there to the end
                //  find the values that got to lastBeforeFirstExecuted and mutate those
                //  identify what they have in common and mutate other stuff
                console.log(`Got nothing after ${lastBeforeFirstExecuted}`);
            } else {
                //  there's a hole in the middle dear liza dear liza
                const gotToFirstOnly: ResultCluster[] = [];
                const gotToBoth: ResultCluster[] = [];

                console.log(`Got hole from ${firstUnexecuted} to ${firstExecutedAfter}`);
                //  find the values that got to lastBeforeFirstExecuted but not firstExecutedAfter and mutate those
                //  X = identify what's common about the values that got to firstExecutedAfter
                //  Y = identify what's common about ALL the values that got to lastBeforeFirstExecuted
                //  mutate the values of X in a way that is not similar to Y
            }
        }
    }

}

/*

bisection - find two parameter lists that are very similar to each other but lead to different code paths
    //  for each parameter list in a cluster, find the outermost
    //  optimization: record which parameter lists are NOT near the edges of their cluster to avoid reexamining
    //  for each pair of outermosts across all cluster, bisect
*/
async function knead(evaluateSpecimen: (b: BaseSpecimen) => Promise<string | undefined>, clustersByKey: Map<string, ResultCluster>, parameterDeclarations: ts.NodeArray<ts.ParameterDeclaration>) {


    if (clustersByKey.size === 0) {
        throw new Error(`No clusters to breed`);
    }

    const clusters = Array.from(clustersByKey.values());
    for (let index = 0; index < parameterDeclarations.length; index++) {

        const required = !parameterDeclarations[index].questionToken;

        for (let i = 0; i < clusters.length - 1; i++) {
            const a = clusters[i];
            const b = clusters[i + 1];

            a.results.sort(comparameters);
            b.results.sort(comparameters);

            const alast = a.results[a.results.length - 1];
            const bfirst = b.results[0];

            const distance = computeDistance(alast.parameters[index], bfirst.parameters[index]);

            const aToB = a.distancesToClusters[index].get(b.key) ?? Infinity;
            if (distance < aToB) {
                a.distancesToClusters[index].set(b.key, distance);
                b.distancesToClusters[index].set(a.key, distance);
            }

            if (distance <= 1) {
                //  found the edges or close enough
                // console.log(`found edges ${distance} between ${JSON.stringify(alast.parameters[index])} and ${JSON.stringify(bfirst.parameters[index])}`);
                continue;
            }
            // console.log(`distance ${distance} between ${JSON.stringify(alast.parameters[index])} and ${JSON.stringify(bfirst.parameters[index])}`);

            //  generate a parameter list where every parameter is hybridized between alast and bfirst
            const hybridized = hybridize(alast.parameters, bfirst.parameters);
            for (const fullHybrid of hybridized) {
                //  also generate a parameter list based on alast with just the current parameter hybridized
                const abased = [...alast.parameters];
                abased[index] = fullHybrid[index];

                //  also generate a parameter list based on bfirst with just the current parameter hybridized
                const bbased = [...bfirst.parameters];
                bbased[index] = fullHybrid[index];

                for (const hybridParams of [fullHybrid, abased, bbased]) {
                    await evaluateSpecimen({
                        parameters: hybridParams,
                        type: 'hybrid',
                        parents: [alast.specimenId, bfirst.specimenId],
                    });
                }
            }
        }
    }
}

async function weed(evaluateSpecimen: (b: BaseSpecimen) => Promise<string | undefined>, maxShrinkGenerations: number, clustersByKey: Map<string, ResultCluster>, parameterDeclarations: ts.NodeArray<ts.ParameterDeclaration>, specimensById: Map<string, Specimen>, supervisor: Supervisor) {
    const pendingShrinkings = new Set<string>();
    const enshrinken = async (baseParameters: any[], toShrinkParameterIndex: number, baseSpecimenId: string) => {
        const specimenIds: string[] = [];
        for (const thisParameterValues of shrink(baseParameters[toShrinkParameterIndex])) {
            const parameters = [...baseParameters];
            parameters[toShrinkParameterIndex] = thisParameterValues;

            const specimenId = await evaluateSpecimen({
                type: 'reduction',
                parameters,
                parent: baseSpecimenId,
            });
            if (specimenId) {
                specimenIds.push(specimenId);
            }
        }
        return specimenIds;
    };

    for (let i = 0; i < maxShrinkGenerations; i++) {
        for (const cluster of clustersByKey.values()) {
            //  start us off
            let batch = 0;
            for (let i = 0; i < parameterDeclarations.length; i++) {
                cluster.results.sort((a, b) => {
                    //  sort by the key parameter first
                    const avalue = a.parameters[i];
                    const bvalue = b.parameters[i];
                    const core = comparameters(avalue, bvalue);
                    if (core !== 0) {
                        return core;
                    }
                    //  then all the rest if necessary
                    for (let j = 0; j < a.parameters.length && j < b.parameters.length; j++) {
                        if (i === j) {
                            continue;
                        }
                        const avalue = a.parameters[j];
                        const bvalue = b.parameters[j];
                        const sub = comparameters(avalue, bvalue);
                        if (sub !== 0) {
                            return sub;
                        }
                    }
                    return 0;
                });

                const top = cluster.results[0];
                cluster.mosts[i] = specimensById.get(top.specimenId)!;
                const allToShrink = [top];

                //  find the minimal least by climbing from the bottom until
                //  hitting a value that is not a strict extension of the previous
                if (cluster.results.length > 1) {
                    let minimalLeastIndex = cluster.results.length - 1;
                    for (let j = minimalLeastIndex - 1; j >= 0; j--) {
                        const current = cluster.results[j];
                        const minimalLeast = cluster.results[minimalLeastIndex];
                        if (!isStrictExtension(current.parameters[i], minimalLeast.parameters[i])) {
                            break;
                        }
                    }

                    const bottom = cluster.results[minimalLeastIndex];
                    allToShrink.push(bottom);
                    cluster.leasts[i] = specimensById.get(bottom.specimenId)!;
                }

                for (const toShrink of allToShrink) {
                    const specimenIds = await enshrinken(toShrink.parameters, i, toShrink.specimenId);
                    specimenIds.forEach(id => pendingShrinkings.add(id));
                }
            }

            const mvpLeastSpecimenId = await evaluateSpecimen({
                type: 'hybrid',
                parameters: cluster.leasts.map((s, i) => s.parameters[i]),
                parents: cluster.leasts.map(s => s.id),
            });

            if (mvpLeastSpecimenId) {
                pendingShrinkings.add(mvpLeastSpecimenId);
            }
            const mvpMostSpecimenId = await evaluateSpecimen({
                type: 'hybrid',
                parameters: cluster.mosts.map((s, i) => s.parameters[i]),
                parents: cluster.mosts.map(s => s.id),
            });
            if (mvpMostSpecimenId) {
                pendingShrinkings.add(mvpMostSpecimenId);
            }

        }
        await supervisor.drain();
    }
}

/*
if (options?.storageBaseDirectory) {
    console.log(`Saving clusters to ${storageBaseDirectory}`);
    saveClusters(inputFile, storageBaseDirectory, functionName, clusters);
}

console.log(`Finished after ${count} iterations and ${Date.now() - startTime}ms with ${allExecutedLines.size}/${introspectionContext.instrumentedLines.size} lines executed; ${JSON.stringify(source.stats())}`);

const sortNums = (a: number, b: number) => a - b;
return {
    instrumented: Array.from(introspectionContext.instrumentedLines).sort(sortNums),
    executed: Array.from(allExecutedLines).sort(sortNums),
};
*/

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
        numbers: new Set(),
        strings: new Set(),
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
