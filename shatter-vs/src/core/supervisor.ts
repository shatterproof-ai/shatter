import { Worker } from 'worker_threads';

// eslint-disable-next-line @typescript-eslint/naming-convention
export const Outcomes = ['completed', 'error', 'timeout', 'failed'] as const;
export type Outcome = typeof Outcomes[number];

export interface RunResult {
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

export class Supervisor {
    private activeWorkers = new Set<Worker>();
    private count = 0;

    private resultByParameters = new Map<string, RunResult>();
    private attemptedParameters = new Set<string>();

    private timeLimit = 1_000;
    constructor(
        private nodePath: string[],
        private executorScriptJS: string,
        private onResult: (result: RunResult) => void,
        private maxActiveWorkers: number) {
    }

    async launchWorker(functionName: string, parameters: any[]) {

        const start = Date.now();
        while (this.activeWorkers.size > this.maxActiveWorkers) {
            //  sort of busy waiting
            // console.log(`Waiting with ${activeWorkers.size} active workers`)
            await new Promise((resolve) => setTimeout(resolve, 1000));
            if (Date.now() - start > maxWaitForWorkerTime) {
                throw new Error(`Timed out waiting for workers to finish`);
            }
        }

        const currentWorkerNumber = ++this.count;
        const workerData = {
            filePath: this.executorScriptJS, functionName, parameters, currentWorkerNumber
        };

        const strung = JSON.stringify(parameters);
        if (this.attemptedParameters.has(strung)) {
            return;
        }

        const NODE_PATH = this.nodePath.join(':');
        // console.log(`attempting ${currentWorkerNumber}:${this.executorScriptJS} with NODE_PATH ${NODE_PATH} and workerData = ${JSON.stringify(workerData)}`);
        console.log(`attempting ${strung}`);
        const worker = new Worker(this.executorScriptJS, {
            workerData,
            stdout: true,
            stderr: true,
            env: {
                // eslint-disable-next-line @typescript-eslint/naming-convention
                NODE_PATH,
            }
        });

        worker.stderr.on('data', (data) => {
            //  TODO: do nothing for now
        });

        worker.stderr.on('error', () => {
            //  TODO: maybe this will be useful at some point?
        });

        worker.stdout.on('data', (data) => {
            //  TODO: maybe this will be useful at some point?
        });

        this.activeWorkers.add(worker);

        const launched = Date.now();
        const timeoutId = setTimeout(() => {
            const timedOut = Date.now();
            if (!this.activeWorkers.has(worker)) {
                //  in case timeout hasn't been cleared for some reason
                return;
            }
            const duration = timedOut - launched;
            // console.log(`Timeout after ${elapsed} ms of ${currentWorkerNumber} with workerData = ${JSON.stringify(workerData)}`)
            worker.terminate();
            this.activeWorkers.delete(worker);
            //  don't overwrite a previous run
            //  TODO: do overwrite if the previous run timed out, but limit the number of times
            if (!this.resultByParameters.has(strung)) {
                const result: RunResult = {
                    parameters, output: undefined, completed: false, duration, executedBranches: [], outcome: 'timeout',
                    lines: [], linesInOrder: [],
                };
                this.resultByParameters.set(strung, result);
                this.onResult(result);
            }
        }, this.timeLimit);

        worker.on('error', (error) => {
            clearTimeout(timeoutId);
            console.log(`Worker ${currentWorkerNumber} for ${functionName} errored ${error}...`);
            worker.terminate();
            this.activeWorkers.delete(worker);

            const duration = Date.now() - launched;
            //  TODO: 
            const strungError = error ? '' + error : undefined;
            const result: RunResult = {
                parameters, error: strungError, completed: false, duration, executedBranches: [], outcome: 'failed', lines: [], linesInOrder: [],
            };

            this.resultByParameters.set(strung, result);

            this.onResult(result);

            throw error;
        });
        worker.on('exit', () => {
            clearTimeout(timeoutId);
            console.log(`Worker ${currentWorkerNumber} for ${functionName} exiting of ${this.activeWorkers.size} running...`);
            this.activeWorkers.delete(worker);
            // console.log(`after deleting ${activeWorkers.size}`);
        });
        worker.on('message', (msg) => {
            const { output, error, duration, executedBranches, lines, linesInOrder }: { output: any, error: any, duration: number, executedBranches: string[], lines:number[], linesInOrder:number[] } = msg;

            // console.log(`${currentWorkerNumber}  ${functionName} (${JSON.stringify(parameters)}) => ${error ?? JSON.stringify(output)} in ${duration}ms`);

            // console.log(`And executed branches = `)
            const strungError = error ? '' + error : undefined;
            const result: RunResult = {
                parameters, output, error: strungError, completed: true, duration, executedBranches, outcome: error ? 'error' : 'completed',
                lines, linesInOrder,
            };

            this.resultByParameters.set(strung, result);

            this.onResult(result);
        });
        return worker;
    }

    async drain(timeout = 10_000) {
        const start = Date.now();
        console.log("finishied draining");
        while (this.activeWorkers.size > 0) {
            //  sort of busy waiting
            // console.log(`Waiting with ${activeWorkers.size} active workers`)
            await new Promise((resolve) => setTimeout(resolve, 100));
            if (Date.now() - start > timeout) {
                console.error(`Timed out waiting for workers to finish`);
                return;
            }
        }
        console.log("finishied draining");
    }
}
