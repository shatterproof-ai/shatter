import { Worker } from 'worker_threads';
import { Invocation, InvocationMeta, InvocationResult, WorkerSetup } from './worker-protocol';

// eslint-disable-next-line @typescript-eslint/naming-convention
export const Outcomes = ['completed', 'error', 'timeout', 'failed'] as const;
export type Outcome = typeof Outcomes[number];

export interface RunResult {
    specimenId: string
    functionName: string
    parameters: any[]
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
}

export class Supervisor {
    private busyWorkers = new Map<number, string>();
    private availableWorkers = new Set<number>();
    private workers = new Map<number, WorkerMeta>();
    private count = 0;

    private resultByInvocation = new Map<string, RunResult>();
    private attemptedInvocations = new Set<string>();

    private invocationsPerWorker = new Map<number, number>();
    private invocationMetaSpecimen = new Map<string, InvocationMeta>();

    private timeLimit = 1_000;
    constructor(
        private nodePath: string[],
        private executorScriptJS: string,
        private maxActiveWorkers: number) {
    }

    async launchWorker(functionName: string, specimenId: string, parameters: any[], onCompletion: (_: Invocation, __: RunResult) => void) {
        const invocation: Invocation = {
            functionName, parameters,
        };

        const strung = JSON.stringify(invocation);
        if (this.attemptedInvocations.has(strung)) {
            return;
        }

        const start = Date.now();
        while (this.busyWorkers.size > this.maxActiveWorkers
            && this.availableWorkers.size === 0) {
            //  sort of busy waiting
            // console.log(`Waiting with ${activeWorkers.size} active workers`)
            await new Promise((resolve) => setTimeout(resolve, 1000));
            if (Date.now() - start > maxWaitForWorkerTime) {
                throw new Error(`Timed out waiting for workers to finish`);
            }
        }

        const stopWorker = (wm: WorkerMeta, outcome: 'expired' | 'timeout' | 'error', error?: any) => {
            clearTimeout(timeoutId);
            const specimenId = this.busyWorkers.get(wm.workerNumber)!;
            this.busyWorkers.delete(wm.workerNumber);
            wm.worker.terminate();

            if (outcome !== 'expired') {
                const meta = this.invocationMetaSpecimen.get(specimenId)!;
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

                //  don't overwrite previous results because they may be good
                //  in case somehow we executed a duplicate
                if (!this.resultByInvocation.has(strung)) {
                    this.resultByInvocation.set(strung, result);
                }

                onCompletion(meta.invocation, result);
            }
        };

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
            const currentWorkerNumber = ++this.count;
            const workerData: WorkerSetup = {
                filePath: this.executorScriptJS, workerNumber: currentWorkerNumber,
            };

            const NODE_PATH = this.nodePath.join(':');

            // console.log(`attempting ${currentWorkerNumber}:${this.executorScriptJS} with NODE_PATH ${NODE_PATH} and workerData = ${JSON.stringify(workerData)}`);
            console.log(`attempting ${currentWorkerNumber} => ${strung}`);
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
                stopWorker(worker, "error", error);
                throw error;
            });
            console.log(`adding exit handler to ${currentWorkerNumber}`);
            newWorker.on('exit', () => {
                clearTimeout(timeoutId);
                // console.log(`Worker ${currentWorkerNumber} for ${functionName} exiting of ${this.activeWorkers.size} running...`);
                this.busyWorkers.delete(worker.workerNumber);   //  necessary in case of some anomalous exit that isn't triggered by a message
                this.workers.delete(worker.workerNumber);
                // console.log(`after deleting ${activeWorkers.size}`);
            });
            newWorker.on('message', (msg) => {
                clearTimeout(timeoutId);
                const { specimenId, output, error, duration, executedBranches, lines, linesInOrder }: InvocationResult = msg;

                this.busyWorkers.delete(worker.workerNumber);

                const invocationCount = this.invocationsPerWorker.get(worker.workerNumber);
                if (invocationCount === undefined || invocationCount >= maxInvocationsPerWorker) {
                    if (invocationCount === undefined) {
                        console.error(`No invocations for worker ${worker.workerNumber}; tidying up`);
                    }
                    stopWorker(worker, 'expired');
                } else {
                    this.availableWorkers.add(worker.workerNumber);
                }

                const meta = this.invocationMetaSpecimen.get(specimenId);
                if (!meta) {
                    console.error(`Unable to find invocation meta for specimen ${specimenId}`);
                    return;
                }
                // console.log(`Worker ${worker.workerNumber} for ${meta.invocation.functionName} completed`);

                // console.log(`And executed branches = `)
                const strungError = error ? '' + error : undefined;
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

                this.resultByInvocation.set(strung, result);

                onCompletion(meta.invocation, result);
            });

            const wwmm: WorkerMeta = {
                filePath: this.executorScriptJS,
                worker: newWorker,
                workerNumber: currentWorkerNumber,
            };

            this.workers.set(currentWorkerNumber, wwmm);
            return wwmm;
        })();

        this.busyWorkers.set(worker.workerNumber, specimenId);
        this.invocationsPerWorker.set(worker.workerNumber, (this.invocationsPerWorker.get(worker.workerNumber) ?? 0) + 1);

        const meta: InvocationMeta = {
            specimenId,
            invocation,
            launched: Date.now(),
        };

        this.invocationMetaSpecimen.set(specimenId, meta);

        // console.log(`invoking worker ${worker.workerNumber}: ${worker.worker}`);
        worker.worker.postMessage(meta);

        const timeoutId = setTimeout(() => {
            if (!this.busyWorkers.has(worker.workerNumber)) {
                //  in case timeout hasn't been cleared for some reason
                return;
            }
            stopWorker(worker, "timeout");
        }, this.timeLimit);
        return worker.workerNumber;
    }

    async drain(timeout = 2_000) {
        const start = Date.now();
        // console.log("finishied draining");
        const waitSome = async (delay: number, max: number) => {
            let count = 0;
            while (this.busyWorkers.size > 0 && count++ < max) {
                //  sort of busy waiting
                // console.log(`Waiting with ${activeWorkers.size} active workers`)
                await new Promise((resolve) => setTimeout(resolve, delay));
                if (Date.now() - start > timeout) {
                    console.error(`Timed out waiting for workers to finish`);
                    return;
                }
            }
        };

        console.log(`Draining with ${this.workers.size} workers`);
        const waitDuration = 100;
        await waitSome(waitDuration, timeout/waitDuration);
        console.log(`Finished draining after ${Date.now() - start} ms with ${this.workers.size}`);
    }

    async terminate(timeout = 10_000) {
        const waitSome = async (delay: number, max: number) => {
            let count = 0;
            const start = Date.now();
            while (this.workers.size > 0 && count++ < max) {
                //  sort of busy waiting
                // console.log(`Waiting with ${activeWorkers.size} active workers`)
                await new Promise((resolve) => setTimeout(resolve, delay));
                if (Date.now() - start > timeout) {
                    console.error(`Timed out waiting for workers to finish`);
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
        await waitSome(waitDuration, timeout/waitDuration);
        // console.log("finishied draining");
    }
}
