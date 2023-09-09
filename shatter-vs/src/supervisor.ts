import { Worker } from 'worker_threads';
import { IntrospectionContext } from './transform';


export interface RunResult {
    parameters: any[]
    executedBranches: string[]
    completed: boolean
    output?: any
    error?: any
    duration: number
}

const maxWaitForWorkerTime = 10_000;

export class Supervisor {
    private activeWorkers = new Set<Worker>();
    private count = 0;

    private resultByParameters = new Map<string, RunResult>();
    private allResults: RunResult[] = [];
    private attemptedParameters = new Set<string>();
    private allExecutedBranches = new Set<string>();

    private timeLimit = 1_000;
    constructor(
        private nodePath: string[],
        private introspectionContext: IntrospectionContext,
        private executorScriptJS: string,
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

        console.log(`attempting ${currentWorkerNumber}:${this.executorScriptJS} with workerData = ${JSON.stringify(workerData)}`);
        const worker = new Worker(this.executorScriptJS, {
            workerData,
            env: {
                // eslint-disable-next-line @typescript-eslint/naming-convention
                NODE_PATH: this.nodePath.join(':')
            }
        });

        const strung = JSON.stringify(parameters);
        if (this.attemptedParameters.has(strung)) {
            return;
        }


        const launched = Date.now();
        const timeoutId = setTimeout(() => {
            const timedOut = Date.now();
            if (!this.activeWorkers.has(worker)) {
                //  in case timeout hasn't been cleared for some reason
                return;
            }
            const elapsed = timedOut - launched;
            // console.log(`Timeout after ${elapsed} ms of ${currentWorkerNumber} with workerData = ${JSON.stringify(workerData)}`)
            worker.terminate();
            this.activeWorkers.delete(worker);
            if (!this.resultByParameters.has(strung)) {
                const result = {
                    parameters, output: undefined, completed: false, duration: -1, executedBranches: []
                };
                this.resultByParameters.set(strung, result);
                this.allResults.push(result);
            }
        }, this.timeLimit);

        worker.on('error', (err) => {
            clearTimeout(timeoutId);
            // console.log(`Worker ${currentWorkerNumber} for ${functionName} errored ${err}...`);
            worker.terminate();
            this.activeWorkers.delete(worker);
            throw err;
        });
        worker.on('exit', () => {
            clearTimeout(timeoutId);
            // console.log(`Worker ${currentWorkerNumber} for ${functionName} exiting of ${activeWorkers.size} running...`);
            this.activeWorkers.delete(worker);
            // console.log(`after deleting ${activeWorkers.size}`);
        });
        worker.on('message', (msg) => {
            const { output, error, duration, executedBranches }: { output: any, error: any, duration: number, executedBranches: string[] } = msg;
            // console.log(`${currentWorkerNumber}  ${functionName} (${JSON.stringify(parameters)}) => ${error ?? JSON.stringify(output)} in ${duration}ms`)

            // console.log(`And executed branches = `)
            this.introspectionContext.knownBranches.forEach((statement, id) => {
                if (!statement) {
                    console.log(`statement is null for id ${id}`);
                    return;
                }

                executedBranches.forEach((id) => this.allExecutedBranches.add(id));
            });

            const strungError = error ? '' + error : undefined;
            const result: RunResult = {
                parameters, output, error: strungError, completed: true, duration, executedBranches
            };

            this.allResults.push(result);
            this.resultByParameters.set(strung, result);

            //  TODO: compare against existing results; generate new inputs if we haven't isolated a split

        });
    }

    drain(timeout=10_000) {
        const start = Date.now();
        while (this.activeWorkers.size > 0) {
            //  sort of busy waiting
            // console.log(`Waiting with ${activeWorkers.size} active workers`)
            setTimeout(() => { }, 100);
            if (Date.now() - start > timeout) {
                console.error(`Timed out waiting for workers to finish`);
                return;
            }
        }
    }

}
