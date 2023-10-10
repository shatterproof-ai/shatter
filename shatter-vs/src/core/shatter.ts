import { createHash } from 'crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, statSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { dirname, join } from 'path';
import * as ts from 'typescript';
import { AbsolutePath, BaseSpecimen, GeneratedParameter, LeafParameter, RelativePath, Specimen, SpecimenId, extractGeneratedParameterValue, findLeaves, isSpecimenId, mergePath, newId, skip } from './common';
import { hybridize, isStrictExtension, shrink } from './hybridize';
import { Outcome, RunResult, Supervisor } from './supervisor';
import { IntrospectionContext, createInstrumenter } from './transform';
import { canonicallyStringify, comparameters, computeDistance, wrapAsync } from './util';
import { Invocation } from './worker-protocol';
import { result } from 'lodash';
import { RetestCaseSource, RuntimeContext, CombinatorialTestCaseSource } from './generator';

export interface AutotestResults {
    //  TODO: make clusters a Record<ClusterKey, SpecimenId[]> and results Record<SpecimenId, RunResult> (eventually to be RunResult[])
    clusters: ResultCluster[];
    instrumentedLines: number[];    //	number[] because Set is not serializable
}

export interface BasicResultCluster {
    key: string
    lines: number[]
    outcome: Outcome
}

interface ResultClusterData {
    //  includes potential duplicates if the same line is hit twice
    specimens: Specimen[]
    results: RunResult[]
}

//  TODO: for error cases add the file and line of where it was thrown and also
//  the file and line of the first line in the instrumented code
export interface ResultCluster extends ResultClusterData, BasicResultCluster {
    //  TODO: this could vary by test case
    linesInOrder: number[]
    //  one to one with parameter list
    leasts: Specimen[]
    //  one to one with parameter list
    mosts: Specimen[]
    totalTime: number
    distancesToClusters: Record<string, number>[]
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


/*

//  custom test cases must be assigned to a cluster

//  intentionally does not use Specimen.id; perhaps that needs to go away entirely in favor of the sha1?

//  where is test case persistence tracked?  this seems like an IDE-specific concern, either tracked entirely in the IDE or via working tree.

autotest
cluster.specimens => Specimen
${storageBaseDirectory}/${path-to-source-file-relative-to-workspace-root}/${functionName}/specimens/custom/${specimenId}.json
${storageBaseDirectory}/${path-to-source-file-relative-to-workspace-root}/${functionName}/specimens/autotest/${specimenId}.json

// TODOTODO: does a cluster have to be a persistent thing?
cluster.results => BasicResultCluster
${storageBaseDirectory}/${path-to-source-file-relative-to-workspace-root}/${functionName}/clusters/${clusterKey}.json

    lines: number[]
    outcome
    
test.results => RunResult
    ${storageBaseDirectory}/${path-to-source-file-relative-to-workspace-root}/${functionName}/results/${specimenId}.json
    
    linesInOrder: number[]
    outcome
*/

/*
//  TODO: should /specimens and /results be near the end or near the beginning of the path?  or in the middle?


//  before, beforeEach, after, afterEach - need some kind of selector to match against file, function, cluster, and test case name
//  look for all *.ts files in the tree at or above the test case inputs file name
//  introduce the concept of a suite?  all test cases in a suite share the same before/after hooks?
//  introduce the concept of environments?  either implicit environment or explicit
//  run all test cases in all environments?  if they error so what?

Don't store all ResultCluster contents, just BasicResultCluster
interface ResultCluster {
    key: string
    lines: number[]
    //  includes potential duplicates if the same line is hit twice
    linesInOrder: number[]
    //  one to one with parameter list
//    leasts: Specimen[]    //  ignore for now; also store as IDs
    //  one to one with parameter list
//    mosts: Specimen[]     //  ignore for now also store as IDs
    outcome: Outcome
//    totalTime: number     //  ignore for now
//    distancesToClusters: Map<string, number>[]    //  ignore for now, perhaps forever
}

    testCaseName default = ${sha1(contents)} OR just a number?  something descriptive for simple cases?  or just random cuid?
        //  do we need to use filenames to enforce uniqueness?  maybe but ignore for now.


*/
export interface RunUpdate {
    batchState: BatchState,
    cluster: ResultCluster,
    specimen: Specimen,
}

const updateBatchState = (batchState: BatchState, runResult: RunResult): RunUpdate => {
    // console.log(`Received result ${JSON.stringify(runResult)}`);
    // find the appropriate cluster or create it
    if (!runResult.specimenId) {
        throw new Error(`No specimenId in ${JSON.stringify(runResult)}`);
    }

    const specimen = batchState.specimensById.get(runResult.specimenId);
    if (!specimen) {
        throw new Error(`No specimen for ${runResult.specimenId}`);
    }

    runResult.lines.forEach(line => batchState.executedLines.add(line));

    const clusterKey = canonicalClusterKey(runResult);
    let cluster = batchState.clustersByKey.get(clusterKey);
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
            specimens: [],
            results: [],
            //  the single specimen has the values that are the least and the most for all parameters
            //  iniitally the only existing specimen is least and most in all cases
            leasts: specimen.parameters.map(_ => specimen),
            mosts: specimen.parameters.map(_ => specimen),
            totalTime: 0,
            distancesToClusters: specimen.parameters.map(_ => ({})),
        };
        batchState.clusters.push(cluster);
        batchState.clustersByKey.set(clusterKey, cluster);
    }

    cluster.specimens.push(specimen);
    cluster.results.push(runResult);
    cluster.totalTime += runResult.duration;

    //  TODO: don't do this on every change
    sortClusters(batchState.clusters);

    return { batchState, cluster, specimen };
};

interface BatchState {
    clusters: ResultCluster[];
    clustersByKey: Map<string, ResultCluster>;
    specimensById: Map<string, Specimen>;
    instrumentedLines: Set<number>;
    executedLines: Set<number>;
}

async function shatterRetestt(modulePaths: string[],
    absoluteSourceInputFile: AbsolutePath,
    functionName: string,
    specimens: Specimen[],
    onUpdate: (update:RunUpdate, results: AutotestResults) => void,
    options?: {
        shatterproofModuleOverride?: string,
        maxIterations?: number,
        maxTime?: number,
        inBand?: boolean,
        maxWorkers?: number,
    }) {

    const [program, sourceFile] = parse(absoluteSourceInputFile);
    const functionDeclarationNode = findFunctionNode(functionName, sourceFile);
    if (!functionDeclarationNode) {
        throw new Error(`Could not find function ${functionName}`);
    }

    // rewrite code of given function (or everything if lazy) to add instrumentation
    const [instrumentedFile, executorScriptJs, introspectionContext] = writeInstrumented(sourceFile, options?.shatterproofModuleOverride);

    console.log(`created ${instrumentedFile} compiled to ${executorScriptJs}`);

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

    const instrumentedLines = Array.from(introspectionContext.instrumentedLines).sort((a, b) => a - b);

    let batchState: BatchState = {
        clusters: [],
        clustersByKey: new Map(),
        specimensById: new Map(),
        instrumentedLines: new Set(introspectionContext.instrumentedLines),
        executedLines: new Set(),
    };

    const onResult = (runResult: RunResult) => {
        const update = updateBatchState(batchState, runResult);
        batchState = update.batchState;
        onUpdate(update, { clusters: batchState.clusters, instrumentedLines });
    };

    const maxWorkers = options?.maxWorkers ?? 15;
    const supervisor = new Supervisor(modulePaths, executorScriptJs, maxWorkers, !!options?.inBand);

    const evaluations: Promise<number | undefined>[] = [];
    const start = Date.now();
    try {
        //  TODO: prioritize variation in simpler types, e.g. numbers, over variation in more complex types, e.g. Maps
        for (const specimen of specimens) {
            batchState.specimensById.set(specimen.id, specimen);
            const e = supervisor.execute(functionName, specimen, (invocation: Invocation, result: RunResult) => {
                onResult(result);
            });
            evaluations.push(e);
        }

        await Promise.all(evaluations);
    } catch (e) {
        console.error(`Error in shatterRetestt: ${e} ${(e as any).stack}`);
    } finally {
        await supervisor.drain();
        await supervisor.terminate();
        const end = Date.now();
        const execution = end - start;

        const executed = Array.from(batchState.executedLines).sort((a, b) => a - b);
        const instrumented = Array.from(introspectionContext.instrumentedLines).sort((a, b) => a - b);

        return { executed, instrumented, clusters: batchState.clusters };
    }
}

export const shatterRetest = wrapAsync("shatterRetestt", shatterRetestt);

/*
    Weirdnessing
    * weirder numbers ???
    * weirder strings ???

    weirdness = 0
    * default

    weirdness = 1
    * don't repeat any exact object or array values

    weirdness = 2
    * don't repeat any leaf values

    TODO: how does weirdness apply to breeding and mutations?
*/


//  operate on the source file instead of editor objects for generality and also to avoid having to duplicate imports
//  TODO: make sure the source file is saved before running
//  TODO: collapse the abstract syntax tree into a tree of conditions and blocks
async function shatterAutotestt(modulePaths: string[],
    absoluteSourceInputFile: AbsolutePath,
    relativeSourceInputFile: RelativePath,
    functionName: string,
    onUpdate: (update:RunUpdate, results: AutotestResults) => void,
    options?: {
        shatterproofModuleOverride?: string,
        maxIterations?: number,
        maxTime?: number,
        inBand?: boolean,
        maxWorkers?: number,
    }
) {
    // parse whole file into abstract syntax tree
    const [program, sourceFile] = parse(absoluteSourceInputFile);
    const functionDeclarationNode = findFunctionNode(functionName, sourceFile);
    if (!functionDeclarationNode) {
        throw new Error(`Could not find function ${functionName}`);
    }

    // rewrite code of given function (or everything if lazy) to add instrumentation
    const [instrumentedFile, executorScriptJs, introspectionContext] = writeInstrumented(sourceFile, options?.shatterproofModuleOverride);

    console.log(`created ${instrumentedFile} compiled to ${executorScriptJs}`);

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

    const maxIterations = options?.maxIterations ?? 200;
    const maxTime = options?.maxTime ?? 15_000;
    const startTime = Date.now();

    const allExecutedBranches = new Set<string>();
    const instrumentedLines = Array.from(introspectionContext.instrumentedLines).sort((a, b) => a - b);

    let batchState: BatchState = {
        clusters: [],
        clustersByKey: new Map(),
        specimensById: new Map(),
        instrumentedLines: new Set(introspectionContext.instrumentedLines),
        executedLines: new Set(),
    };

    const onResult = (runResult: RunResult) => {
        const update = updateBatchState(batchState, runResult);
        batchState = update.batchState;
        onUpdate(update, { clusters: batchState.clusters, instrumentedLines });
    };

    const maxWorkers = options?.maxWorkers ?? 15;
    const supervisor = new Supervisor(modulePaths, executorScriptJs, maxWorkers, !!options?.inBand);
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

    const runtimeContext: RuntimeContext = {
        activeModule: undefined,
        weirdness: 0,
        leafPeeping: new Map(),
    };
    // const generator = new CombinatorialTestCaseSource(program.getTypeChecker(), functionDeclarationNode.parameters);
    const source = new CombinatorialTestCaseSource(program.getTypeChecker(), functionDeclarationNode);
    const seeder = source.seeder(runtimeContext, { numbers: introspectionContext.numbers, strings: introspectionContext.strings });

    const linesImprovedByOperation: Record<'seed' | 'weed' | 'breed' | 'knead', number> = {
        seed: 0,
        weed: 0,
        breed: 0,
        knead: 0
    };

    let count = 0;

    //  SEED
    const seedsPerUnexecutedLine = 4;
    const maxShrinkGenerations = 4;
    const minResultsPerCluster = 3;

    const seen = new Set<string>();

    const seenParameters: Set<any>[] = functionDeclarationNode.parameters.map(_ => new Set());

    //  TODO: this only looks at uniqueness relative to what's been executed and ignores overlapping values in this cohort
    function scorePerParameterUniqueness(specimen: BaseSpecimen) {
        let score = 0;
        for (let index = 0; index < specimen.parameters.length; index++) {
            const seen = seenParameters[index];
            const extracted = extractGeneratedParameterValue(specimen.parameters[index]);
            const strung = canonicallyStringify(extracted);
            if (!seen.has(strung)) {
                seen.add(strung);
                //  TODO: compute cosine similarity against all others seen in this slot and return 1-max(similarity)
                //  go negative so that more similar is higher in the list
                score--;
            }
        }

        return score;
    }

    const specimenLeaves:Record<SpecimenId, LeafParameter[]> = {};
    //  TODO: make this part of the specimen generation
    function scoreByDepth(specimen: Specimen) {
        return specimenLeaves[specimen.id].reduce((maxSeen, leaf) => leaf.path.length > maxSeen ? leaf.path.length : maxSeen, 0);
    }

    const maxSpecimensToConsider = 10_000;
    const linesRemainingPerPass: number[] = [];

    let specimens: Specimen[] = [];
    async function executeStage(name: keyof typeof linesImprovedByOperation, take: number, g: Generator<BaseSpecimen, any, any>, scoringFunction: (specimen: Specimen) => number) {
        // console.log(`${name} ${take}; ${count} done so far`);
        const start = Date.now();

        let i = 0;
        let discarded = 0;
        const beforeLines = batchState.executedLines.size;
        for (const baseSpecimen of g) {
            // console.log(`parameters ${i}/${discarded} ${JSON.stringify(parameters)}`);
            //  TODO: will this have false positive matches on function members?
            //  the alternative is to use serialize-javascript, but that does not
            //  seem to produce a canonical ordering
            const leafValues = baseSpecimen.parameters.flatMap(p => {
                const leaves: LeafParameter[] = [];
                for (const leafGP of findLeaves(p)) {
                    leaves.push({
                        ...leafGP,
                        mergedPath: mergePath(leafGP.path),
                        value: extractGeneratedParameterValue(leafGP),
                    });
                }
                return leaves;
            });

            //  TODO: perhaps in some cases we want to sort by other things,
            //  like depth or length or aggregate size
            leafValues.sort((a, b) => a.mergedPath.localeCompare(b.mergedPath));
            const strung = JSON.stringify(leafValues);
            // const strung = canonicallyStringify(parameters);
            if (strung && !seen.has(strung)) {
                seen.add(strung);

                const specimenId = newId(baseSpecimen.type);

                //  TODO: in Autotest mode we're always creating a new specimen, but in Retest mode we are not
                const specimen: Specimen = {
                    fileUnderTest: relativeSourceInputFile,
                    id: specimenId, //  TODO: this should be either the specimen name or a SHA1 of the specimen parameters (both?)
                    functionName,
                    ...baseSpecimen,
                };
                specimens.push(specimen);
            } else {
                discarded++;
            }

            if (specimens.length > 10 * take || i++ > 50 * take) {
                break;
            }
        }

        const scoredSpecimens = specimens.map(specimen => ({ specimen, score: scoringFunction(specimen) }))
            .sort((a, b) => a.score - b.score)
            .map(ss => ss.specimen);

        const toRun = scoredSpecimens.slice(0, take);

        const betweenGenerationAndExecution = Date.now();

        const evaluations: Promise<string>[] = [];
        for (const newSpecimen of toRun) {
            batchState.specimensById.set(newSpecimen.id, newSpecimen);
            // console.log(`Evaluating specimen ${JSON.stringify(newSpecimen)}`);
            const p = supervisor.execute(functionName, newSpecimen, (invocation: Invocation, result: RunResult) => {
                onResult(result);
            }).then(_ => newSpecimen.id);
            evaluations.push(p);
        }

        console.log(`specimens was ${specimens.length} now ${toRun.length} of ${scoredSpecimens.length} scored with max ${maxSpecimensToConsider}`);
        specimens = scoredSpecimens.slice(take, take + maxSpecimensToConsider);

        return Promise.all(evaluations)
            .then(_ => supervisor.drain())
            .then(_ => {
                const end = Date.now();
                const generation = end - betweenGenerationAndExecution;
                const execution = betweenGenerationAndExecution - start;
                const netLines = beforeLines - batchState.executedLines.size;
                linesImprovedByOperation[name] += netLines;
                console.log(`Round ${linesRemainingPerPass.length} coverage ${batchState.executedLines.size}/${introspectionContext.instrumentedLines.size}: ${name} ${toRun.length}/${take} specimens of ${count} total so far took ${generation}ms to generate and ${execution}ms to execute with ${specimens.length} left over; discarded ${discarded} repeats`);
                return [generation, execution];
            });
    }

    try {
        //  TODO: prioritize variation in simpler types, e.g. numbers, over variation in more complex types, e.g. Maps
        while (count < maxIterations && Date.now() - startTime < maxTime) {
            //  generate at least one specimen per unexecuted line

            const linesRemaining = introspectionContext.instrumentedLines.size - batchState.executedLines.size;
            //  TODO: keep track of which method has most recently been successful
            const oneLineBack = linesRemainingPerPass?.[linesRemainingPerPass.length - 1] ?? introspectionContext.instrumentedLines.size;
            const progress = oneLineBack - linesRemaining;
            const fiveLinesBack = linesRemainingPerPass?.[linesRemainingPerPass.length - 5] ?? introspectionContext.instrumentedLines.size;
            const progress5 = fiveLinesBack - linesRemaining;

            if (progress5 === 0) {
                runtimeContext.weirdness += 2;
            } else if (progress === 0) {
                runtimeContext.weirdness++;
            }

            linesRemainingPerPass.push(linesRemaining);
            //  TODO: if we're not making progress, increase weirdness (HOW??? more unique individual values?)
            const toSeed = Math.min(maxIterations, Math.min(linesRemaining, 10) * seedsPerUnexecutedLine);
            await executeStage("seed", toSeed, seeder, scorePerParameterUniqueness);

            //  WEED - find the smaller ones - this matters less if we have low coverage
            const toWeed = Math.min(maxIterations - count, Math.ceil(batchState.executedLines.size * 0.1 + 10));
            const weeder = weed(maxShrinkGenerations, batchState.clustersByKey, functionDeclarationNode.parameters, batchState.specimensById);
            await executeStage("weed", toWeed, weeder, scoreByDepth);      //  TODO: weed-specific score - estimate size of input in some fashion; smaller is better

            //  BREED - we want more of these if we have holes in our coverage
            const toBreed = Math.min(maxIterations - count, Math.ceil(count * 0.2 + 20));
            const breeder = breed(introspectionContext.instrumentedLines, batchState.executedLines, batchState.clusters);
            await executeStage("breed", toBreed, breeder, scorePerParameterUniqueness);   //  TODO: breed-specific score - some kind of holistic uniqueness?  individual parameters are likely to overlap

            //  KNEAD - find the boundary cases
            //  only do clusters that have distance > 1 from neighbors
            //  and if they've gotten closer recently
            const preKneadRatio = batchState.executedLines.size / introspectionContext.instrumentedLines.size;
            const toKnead = Math.min(maxIterations - count, functionDeclarationNode.parameters.length * (1 + batchState.clusters.length) * batchState.executedLines.size * (preKneadRatio + 0.01));
            const kneader = knead(batchState.clustersByKey, functionDeclarationNode.parameters, batchState.specimensById);
            await executeStage("knead", toKnead, kneader, scorePerParameterUniqueness);   //  TODO: knead-specific score - distance from centroid of cluster?

            if (introspectionContext.instrumentedLines.size - batchState.executedLines.size === 0) {
                //  TODO: continue until at least N examples in each cluster
                const incompleteClusters = batchState.clusters.filter(c => c.results.length < minResultsPerCluster);
                if (incompleteClusters.length === 0) {
                    break;
                }
            }
        }
    } catch (e) {
        console.error(`Error in shatterAutotestt: ${e} ${(e as any).stack}`);
    } finally {

        await supervisor.terminate();

        const executed = Array.from(batchState.executedLines).sort((a, b) => a - b);
        const instrumented = Array.from(introspectionContext.instrumentedLines).sort((a, b) => a - b);

        console.log(`Finished after ${count} iterations and ${Date.now() - startTime}ms with ${batchState.executedLines.size}/${introspectionContext.instrumentedLines.size} lines executed; ${JSON.stringify(linesImprovedByOperation)}`);

        return { count, executed, instrumented, clusters: batchState.clusters };
    }
}

export const shatterAutotest = wrapAsync("shatterAutotestt", shatterAutotestt);

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
function* breed(allInstrumentedLines: Set<number>, allExecutedLines: Set<number>, _clusters: ResultCluster[]): Generator<Specimen, any, any> {


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
function* knead(clustersByKey: Map<string, ResultCluster>, parameterDeclarations: ts.NodeArray<ts.ParameterDeclaration>,
    specimens: Map<string, Specimen>) {

    if (clustersByKey.size === 0) {
        throw new Error(`No clusters to breed`);
    }

    const clusters = Array.from(clustersByKey.values())
        //  don't bother to hybridize errors, timeouts, or failures; the most we want to do there is bisect
        .filter(c => c.outcome === 'completed');
    for (let index = 0; index < parameterDeclarations.length; index++) {
        for (let i = 0; i < clusters.length - 1; i++) {
            const a = clusters[i];
            const b = clusters[i + 1];

            a.results.sort(comparameters);
            b.results.sort(comparameters);

            const alast = a.results[a.results.length - 1];
            const bfirst = b.results[0];

            const specimenA = specimens.get(alast.specimenId)!;
            const specimenB = specimens.get(bfirst.specimenId)!;

            const distance = computeDistance(specimenA.parameters[index], specimenB.parameters[index]);

            const aToB = a.distancesToClusters[index][b.key] ?? Infinity;
            if (distance < aToB) {
                a.distancesToClusters[index][b.key] = distance;
                b.distancesToClusters[index][a.key] = distance;
            }

            if (distance <= 1) {
                //  found the edges or close enough
                // console.log(`found edges ${distance} between ${JSON.stringify(alast.parameters[index])} and ${JSON.stringify(bfirst.parameters[index])}`);
                continue;
            }
            // console.log(`distance ${distance} between ${JSON.stringify(alast.parameters[index])} and ${JSON.stringify(bfirst.parameters[index])}`);

            const arbitraryListLimit = 5;
            const arbitraryParameterVariationLimit = 4;
            for (let i = 0; i < arbitraryListLimit; i++) {
                const parameters: GeneratedParameter[] = [];
                for (let j = 0; j < specimenA.parameters.length; j++) {
                    const paramA = specimenA.parameters[j];
                    const paramB = specimenB.parameters[j];
                    const hybridized = hybridize(paramA, paramB);
                    let k = 0;

                    const backupValue = (i % 2 === 0) ? paramA : paramB;
                    const p: GeneratedParameter = skip(hybridized, i + j) ?? backupValue;
                    parameters.push(p);
                }

                //  the parameter list may be a repeat, but that'll get dealt with downstream
                const specimen: BaseSpecimen = {
                    type: 'hybrid',
                    parameters,
                    parents: [specimenA.id, specimenB.id],
                };
                yield specimen;
            }
        }
    }
}

function* weed(maxShrinkGenerations: number, clustersByKey: Map<string, ResultCluster>, parameterDeclarations: ts.NodeArray<ts.ParameterDeclaration>, specimensById: Map<string, Specimen>) {
    for (let i = 0; i < maxShrinkGenerations; i++) {
        for (const cluster of clustersByKey.values()) {
            //  start us off
            let batch = 0;
            for (let i = 0; i < parameterDeclarations.length; i++) {
                cluster.results.sort((a, b) => {
                    //  sort by the key parameter first
                    const specimenA = specimensById.get(a.specimenId)!;
                    const specimenB = specimensById.get(b.specimenId)!;

                    const avalue = specimenA.parameters[i];
                    const bvalue = specimenB.parameters[i];
                    const core = comparameters(avalue, bvalue);
                    if (core !== 0) {
                        return core;
                    }
                    //  then all the rest if necessary
                    for (let j = 0; j < specimenA.parameters.length && j < specimenB.parameters.length; j++) {
                        if (i === j) {
                            continue;
                        }
                        const avalue = specimenA.parameters[j];
                        const bvalue = specimenB.parameters[j];
                        const sub = comparameters(avalue, bvalue);
                        if (sub !== 0) {
                            return sub;
                        }
                    }
                    return 0;
                });

                const top = cluster.results[0];
                const topSpecimen = specimensById.get(top.specimenId)!;
                cluster.mosts[i] = topSpecimen;
                const allToShrink = [topSpecimen];

                //  find the minimal least by climbing from the bottom until
                //  hitting a value that is not a strict extension of the previous
                if (cluster.results.length > 1) {
                    let minimalLeastIndex = cluster.results.length - 1;
                    let specimenMinimalLeast = specimensById.get(cluster.results[minimalLeastIndex].specimenId)!;
                    for (let j = minimalLeastIndex - 1; j >= 0; j--) {
                        const current = cluster.results[j];
                        const specimenCurrent = specimensById.get(current.specimenId)!;

                        if (!isStrictExtension(specimenCurrent.parameters[i], specimenMinimalLeast.parameters[i])) {
                            break;
                        }
                    }

                    const bottomResult = cluster.results[minimalLeastIndex];
                    const bottomSpecimen = specimensById.get(bottomResult.specimenId)!;
                    allToShrink.push(bottomSpecimen);
                    cluster.leasts[i] = bottomSpecimen;
                }

                for (const toShrink of allToShrink) {
                    const baseParameters = toShrink.parameters;
                    const toShrinkParameterIndex = i;
                    const baseSpecimenId = toShrink.id;
                    const specimenIds: string[] = [];
                    for (const thisParameterValues of shrink(baseParameters[toShrinkParameterIndex])) {
                        const parameters = [...baseParameters];
                        parameters[toShrinkParameterIndex] = thisParameterValues;

                        const r: BaseSpecimen = {
                            type: 'reduction',
                            parameters,
                            parent: baseSpecimenId,
                        };
                        yield r;
                    }

                }
            }

            const h1: BaseSpecimen = {
                type: 'hybrid',
                parameters: cluster.leasts.map((s, i) => s.parameters[i]),
                parents: cluster.leasts.map(s => s.id),
            };
            yield h1;

            const h2: BaseSpecimen = {
                type: 'hybrid',
                parameters: cluster.mosts.map((s, i) => s.parameters[i]),
                parents: cluster.mosts.map(s => s.id),
            };
            yield h2;
        }
    }
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
            a.serializedParameterValues.localeCompare(b.serializedParameterValues)
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
        throw new Error(`Could not find source file for function ${sourceFilePath}`);
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

    const projectCompilerOptions: ts.CompilerOptions = (() => {
        const configFileName = ts.findConfigFile(
            "./",
            ts.sys.fileExists,
            "tsconfig.json"
        );

        if (!configFileName) {
            return {};
        }

        const configFile = ts.readConfigFile(configFileName, ts.sys.readFile);
        return ts.parseJsonConfigFileContent(
            configFile.config,
            ts.sys,
            "./"
        );
    })();

    const compilerOptions: ts.CompilerOptions = {
        module: ts.ModuleKind.CommonJS,
        target: ts.ScriptTarget.ES2020,
        ...projectCompilerOptions,
        // isolatedModules: true,  //  TODO: is this necessary?
        inlineSourceMap: true,
        sourceMap: true,
    };

    const codeTransformer = createInstrumenter(introspectionContext, shatterproofModuleOverride);
    const transformed = ts.transform(sourceFile, [codeTransformer], compilerOptions);

    const tempdir = mkdtempSync(join(tmpdir(), "shatterproof-"));
    const printer = ts.createPrinter({ newLine: ts.NewLineKind.LineFeed });

    const modifiedSourcefilePath = join(tempdir, 'temp.ts');

    const transformedSource = printer.printNode(ts.EmitHint.Unspecified, transformed.transformed[0], transformed.transformed[0]);
    writeFileSync(modifiedSourcefilePath, transformedSource);

    const startingSource = readFileSync(sourceFile.fileName, 'utf8');
    const transpilationOutput = ts.transpileModule(startingSource, {
        fileName: modifiedSourcefilePath,
        transformers: {
            before: [codeTransformer]
        },
        compilerOptions,
    });

    const executorScriptJs = modifiedSourcefilePath.replace(/\.tsx?$/, '.js');
    const executorScriptJs2 = modifiedSourcefilePath.replace(/\.tsx?$/, '2.js');
    writeFileSync(executorScriptJs2, transpilationOutput.outputText);

    const modifiedProgram = ts.createProgram([modifiedSourcefilePath], compilerOptions);
    const modifiedSource = modifiedProgram.getSourceFile(modifiedSourcefilePath);
    if (!modifiedSource) {
        throw new Error(`Could not find source file ${modifiedSourcefilePath}`);
    }
    //  TODO: how to know what the filename is?  Is that what writeFileCallback does?
    //  Or does that replace the file writing that would otherwise happen?
    modifiedProgram.emit();

    //  write a new version of the function with instrumentation
    //  replace it in the AST
    return [modifiedSourcefilePath, executorScriptJs, introspectionContext];
}
