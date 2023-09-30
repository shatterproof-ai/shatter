import { Worker } from 'worker_threads';
import { Invocation, InvocationMeta, InvocationResult, WorkerSetup } from './worker-protocol';
import { GeneratedParameter, extractGeneratedParameterValue } from './common';

import serializeJavascript = require("serialize-javascript");
import { execute, work } from './worker';
import { basename, dirname, join } from 'path';
import { symlinkSync } from 'fs';
import { Specimen } from './generator';

// eslint-disable-next-line @typescript-eslint/naming-convention
export const Outcomes = ['completed', 'error', 'timeout', 'failed'] as const;
export type Outcome = typeof Outcomes[number];

export interface RunResult {
    specimenId: string
    functionName: string
    serializedParameterValues: string
    executedBranches: string[]
    lines: number[]
    linesInOrder: number[]
    completed: boolean
    outcome: Outcome
    output?: any
    error?: any
    duration: number
    stdout?: string
    stderr?: string
}

const maxWaitForWorkerTime = 10_000;
const maxInvocationsPerWorker = 200;

interface WorkerMeta extends WorkerSetup {
    worker: Worker
    invocations: number
    timeoutId?: NodeJS.Timeout
}

const canonicalizeInvocation = (meta: InvocationMeta) => {
    const resolvedParameters = meta.generatedParameters.map(extractGeneratedParameterValue);
    const canonicalized = serializeJavascript({
        functionName: meta.invocation.functionName,
        resolvedParameters
    });

    return canonicalized;
};

export class Supervisor {
    private busyWorkers = new Map<number, string>();
    private availableWorkers = new Set<number>();
    private workers = new Map<number, WorkerMeta>();
    private count = 0;

    private resultBySpecimen = new Map<string, RunResult>();
    //  TODO: this may not be the place to prevent reruns; perhaps the caller?
    private attemptedInvocations = new Set<string>();

    private invocationMetaSpecimen = new Map<string, InvocationMeta>();

    private timeLimit = 1_000;
    constructor(
        private nodePath: string[],
        private executorScriptJS: string,
        private maxActiveWorkers: number,
        private inBand: boolean,
    ) {
        //  TODO: not just in band?
        if (this.inBand) {
            if (this.nodePath.length !== 1) {
                throw new Error(`In-band execution can only have one directory in node path but got ${this.nodePath}`);
            }
            const realNodeModulesPath = this.nodePath[0];
            const linkedNodeModulesPath = join(dirname(this.executorScriptJS), 'node_modules');
            symlinkSync(realNodeModulesPath, linkedNodeModulesPath);
        }
    }

    processInvocationResult(invocationResult: InvocationResult, onCompletion: (_: Invocation, __: RunResult) => void) {
        const { specimenId, output, error, duration, executedBranches, lines, linesInOrder }: InvocationResult = invocationResult;

        const meta = this.invocationMetaSpecimen.get(specimenId);
        if (!meta) {
            console.error(`Unable to find invocation meta for specimen ${specimenId}`);
            return;
        }
        // console.log(`Worker ${worker.workerNumber} for ${meta.invocation.functionName} completed`);

        // console.log(`And executed branches = `)
        const strungError = error ? JSON.stringify(error) : undefined;
        const result: RunResult = {
            ...meta.invocation,
            specimenId,
            output,
            error: strungError,
            completed: true,
            duration,
            executedBranches,
            outcome: error ? 'error' : 'completed',
            lines,
            linesInOrder,
        };

        this.resultBySpecimen.set(meta.specimenId, result);

        onCompletion(meta.invocation, result);
    };

    //  why have this separate from purgeWorker?
    onWorkerExit(worker: WorkerMeta,) {
        this.purgeWorker(worker);
    }

    onWorkerMessage(invocationResult: InvocationResult, worker: WorkerMeta, onCompletion: (_: Invocation, __: RunResult) => void) {
        // console.log(`received message ${JSON.stringify(invocationResult)}`);
        // const invocationResult:InvocationResult = eval(msg);
        clearTimeout(worker.timeoutId);
        worker.timeoutId = undefined;
        if (worker.invocations >= maxInvocationsPerWorker) {
            this.purgeWorker(worker);
        } else {
            this.availableWorkers.add(worker.workerNumber);
        }

        this.processInvocationResult(invocationResult, onCompletion);
        this.busyWorkers.delete(worker.workerNumber);
    }

    purgeWorker(wm: WorkerMeta) {
        clearTimeout(wm.timeoutId);
        this.workers.delete(wm.workerNumber);
        this.busyWorkers.delete(wm.workerNumber);
        this.availableWorkers.delete(wm.workerNumber);
        wm.worker.terminate();
    }

    stopWorker(wm: WorkerMeta, outcome: 'timeout' | 'error', onCompletion: (_: Invocation, __: RunResult) => void, error?: any) {
        const specimenId = this.busyWorkers.get(wm.workerNumber)!;
        this.purgeWorker(wm);

        const meta = this.invocationMetaSpecimen.get(specimenId);
        if (!meta) {
            console.error(`Unable to find invocation meta for specimen ${specimenId}`);
            return;
        }
        console.log(`Worker ${wm.workerNumber} for ${meta.invocation.functionName} ${outcome} ${error}...`);

        const duration = Date.now() - meta.launched;
        //  TODO: 
        const strungError = error ? '' + error : undefined;
        const result: RunResult = {
            ...meta.invocation,
            specimenId,
            error: strungError,
            completed: false,
            duration,
            executedBranches: [],
            outcome,
            lines: [],
            linesInOrder: [],
        };

        this.resultBySpecimen.set(specimenId, result);

        onCompletion(meta.invocation, result);
    };

    async execute(functionName: string, specimen: Specimen, onCompletion: (_: Invocation, __: RunResult) => void) {
        const resolvedParameters = specimen.parameters.map(extractGeneratedParameterValue);
        const serializedParameterValues = serializeJavascript(resolvedParameters);
        const invocation: Invocation = {
            functionName, serializedParameterValues,
        };

        const meta: InvocationMeta = {
            specimenId: specimen.id,
            invocation,
            generatedParameters: specimen.parameters,
            launched: Date.now(),
        };

        const canonicalizedInvocation = canonicalizeInvocation(meta);
        if (this.attemptedInvocations.has(canonicalizedInvocation)) {
            return;
        }
        this.attemptedInvocations.add(canonicalizedInvocation);

        //  store metadata in a map because workers get reused, so we can't capture the metadata
        //  from the surrounding scope in a closure for the out-of-band version; that only works
        //  for the in-band version or the first run of the out-of-band version
        this.invocationMetaSpecimen.set(specimen.id, meta);

        // console.log(`#${this.count} at ${new Date()} - ${specimen.id} : ${specimen.parameters} is ${JSON.stringify(resolvedParameters)}`);
        // console.log(`${specimen.id} : ${JSON.stringify(specimen.parameters)}`);
        this.count++;

        const start = Date.now();
        process.env.MAIN_PROCESS = '1';

        //  support in-band execution for simpler debugging and viewing of output
        //  (TODO: figure out how to capture worker stdout and stderr)
        if (this.inBand) {
            try {
                const module = await import(this.executorScriptJS);
                const functions = {
                    [functionName]: module[functionName],
                };

                const result = await work(functions, 0, meta);

                this.processInvocationResult(result, onCompletion);

            } catch (e: any) {
                console.error(`Unable to execute ${functionName} in-band: ${e} - ${e.stack}`);
            }
            return;
        }

        while (this.busyWorkers.size > this.maxActiveWorkers
            && this.availableWorkers.size === 0) {
            //  sort of busy waiting
            // console.log(`Waiting with ${activeWorkers.size} active workers`)
            await new Promise((resolve) => setTimeout(resolve, 1000));
            if (Date.now() - start > maxWaitForWorkerTime) {
                throw new Error(`Timed out waiting for workers to finish`);
            }
        }

        const worker: WorkerMeta = (() => {
            if (this.availableWorkers.size > 0) {
                const first = this.availableWorkers.values().next();
                const workerNumber: number = first.value;
                const reworker = this.workers.get(workerNumber);
                if (reworker) {
                    // console.log(`Reusing worker ${reworker}`);
                    this.availableWorkers.delete(workerNumber);

                    return reworker;
                }
                console.error(`Inexplicably unable to find worker ${workerNumber}`);
            }
            const currentWorkerNumber = this.count;
            const workerData: WorkerSetup = {
                filePath: this.executorScriptJS, workerNumber: currentWorkerNumber,
            };

            const NODE_PATH = this.nodePath.join(':');

            // console.log(`attempting ${currentWorkerNumber}:${this.executorScriptJS} with NODE_PATH ${NODE_PATH} and workerData = ${JSON.stringify(workerData)}`);
            // console.log(`attempting ${currentWorkerNumber} => ${strung}`);
            const newWorker = new Worker(this.executorScriptJS, {
                workerData,
                stdout: true,
                stderr: true,
                env: {
                    // eslint-disable-next-line @typescript-eslint/naming-convention
                    NODE_PATH,
                }
            });
            newWorker.stderr.on('data', (data) => {
                //  TODO: do nothing for now
            });

            newWorker.stderr.on('error', () => {
                //  TODO: maybe this will be useful at some point?
            });

            newWorker.stdout.on('data', (data) => {
                //  TODO: maybe this will be useful at some point?
            });

            newWorker.on('error', (error) => {
                //  TODO: be less willing to give up on error; how to identify the recoverable ones?  by stack trace?
                this.stopWorker(worker, "error", onCompletion, error);
                throw error;
            });

            newWorker.on('exit', () => {
                this.onWorkerExit(worker);
            });

            newWorker.on('message', (msg) => {
                this.onWorkerMessage(msg, worker, onCompletion);
            });

            const wwmm: WorkerMeta = {
                filePath: this.executorScriptJS,
                worker: newWorker,
                workerNumber: currentWorkerNumber,
                invocations: 0,
            };

            this.workers.set(currentWorkerNumber, wwmm);
            return wwmm;
        })();

        this.busyWorkers.set(worker.workerNumber, specimen.id);

        worker.timeoutId = setTimeout(() => {
            if (!this.busyWorkers.has(worker.workerNumber)) {
                return;
            }
            this.stopWorker(worker, "timeout", onCompletion);
        }, this.timeLimit);

        // console.log(`invoking worker ${worker.workerNumber}: ${worker.worker}`);
        meta.launched = Date.now(); //  reset because we may have waited for a worker to become available
        worker.worker.postMessage(meta);

        return worker.workerNumber;
    }

    async drain(timeout = 2_000) {
        if (this.inBand) {
            return;
        }
        const start = Date.now();
        // console.log("finishied draining");
        const waitSome = async (delay: number, max: number) => {
            let count = 0;
            while (this.busyWorkers.size > 0 && count++ < max) {
                //  sort of busy waiting
                // console.log(`Waiting with ${activeWorkers.size} active workers`)
                await new Promise((resolve) => setTimeout(resolve, delay));
                if (Date.now() - start > timeout) {
                    console.error(`Timed out waiting for workers ${Array.from(this.busyWorkers.keys()).join(", ")} to finish`);
                    return;
                }
            }
        };

        console.log(`Draining with ${this.workers.size} workers`);
        const waitDuration = 100;
        await waitSome(waitDuration, timeout / waitDuration);
        console.log(`Finished draining after ${Date.now() - start} ms with ${this.workers.size}`);
    }

    async terminate(timeout = 10_000) {
        if (this.inBand) {
            return;
        }
        const waitSome = async (delay: number, max: number) => {
            let count = 0;
            const start = Date.now();
            while (this.workers.size > 0 && count++ < max) {
                //  sort of busy waiting
                // console.log(`Waiting with ${activeWorkers.size} active workers`)
                await new Promise((resolve) => setTimeout(resolve, delay));
                if (Date.now() - start > timeout) {
                    console.error(`Timed out waiting for workers ${Array.from(this.workers.values()).map(w => w.workerNumber).join(", ")} to finish`);
                    return;
                }
            }
        };

        //  TODO: determine appropriate semantics of exit; does it interrupt execution or is it graceful?
        for (const workerMeta of this.workers.values()) {
            console.log(`Terminating worker ${workerMeta.workerNumber}`);
            workerMeta.worker.terminate();
        }

        const waitDuration = 100;
        await waitSome(waitDuration, timeout / waitDuration);
        // console.log("finishied draining");
    }
}
