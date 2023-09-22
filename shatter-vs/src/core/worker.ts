import { parentPort, workerData } from 'worker_threads';
import { ExecutionContext, contextStorage } from './recorder';
import { Invocation, InvocationMeta, InvocationResult, WorkerSetup } from './worker-protocol';

// eslint-disable-next-line @typescript-eslint/ban-types
export async function execute(functions: Record<string, Function>) {
    if (!parentPort) {
        throw new Error("No parent port");
    }
    const definitelyNotNullParentPortToMakeTypescriptHappy = parentPort;

    //  TODO: run this in a loop to allow reuse of threads OR use a thread pool (benchmark overhead first)
    const { filePath, workerNumber }: WorkerSetup = workerData;

    let invocations = 0;

    definitelyNotNullParentPortToMakeTypescriptHappy.on('message', (message) => {
        const { invocation, specimenId }: InvocationMeta = message;
        const { functionName, parameters }: Invocation = invocation;
        const executedBranches = new Set<string>();

        const ic: ExecutionContext = {
            executedBranches,
            branchStack: [],
            lines: new Set<number>(),
            linesInOrder: [],
        };

        const f = functions[functionName];
        contextStorage.run(ic, async () => {
            const start = Date.now();
            let output: any = undefined;
            let error: any = undefined;
            try {
                console.log(`calling ${workerNumber} (${++invocations}) ${functionName} (${JSON.stringify(parameters)})`);
                output = f.call(null, ...parameters);
                // console.log(`called ${currentWorkerNumber} ${functionName} (${JSON.stringify(parameters)}) => ${JSON.stringify(output)}`);
            } catch (e) {
                /* 
                TODO: how to differentiate between types of error
                * well-functioning code, e.g. validation
                * likely bug, e.g. attempting to dereference undefined
                * serious error, e.g. stack overflow
                * crash the VM, e.g. out of memory error
                */
                error = e;
                console.log(`${workerNumber} ${functionName} (${JSON.stringify(parameters)}) threw ${e}`);
            } finally {
                const end = Date.now();
                const duration = end - start;

                const lines = Array.from(ic.lines).sort((a, b) => a - b);
                const result: InvocationResult = {
                    specimenId,
                    output,
                    error,
                    duration,
                    executedBranches: Array.from(executedBranches),
                    lines,
                    linesInOrder: ic.linesInOrder
                };
                definitelyNotNullParentPortToMakeTypescriptHappy.postMessage(result);
            }
        });
    })
}