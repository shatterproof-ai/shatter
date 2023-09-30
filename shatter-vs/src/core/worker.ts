import { parentPort, workerData } from 'worker_threads';
import { extractGeneratedParameterValue } from './common';
import { ExecutionContext, contextStorage } from './recorder';
import { Invocation, InvocationMeta, InvocationResult, WorkerSetup } from './worker-protocol';

export function work(functions: Record<string, Function>, workerNumber: number, message: any) {
    const { invocation, specimenId, generatedParameters }: InvocationMeta = message;
    const { functionName, serializedParameterValues }: Invocation = invocation;

    // console.log(`worker ${workerNumber} executing ${functionName} for ${specimenId}`);
    const resolvedParameters = generatedParameters.map(extractGeneratedParameterValue);

    const executedBranches = new Set<string>();

    const ic: ExecutionContext = {
        executedBranches,
        branchStack: [],
        lines: new Set<number>(),
        linesInOrder: [],
    };

    const f = functions[functionName];
    if (!f) {
        throw new Error(`No function ${functionName} in ${Object.keys(functions)}`);
    }
    return contextStorage.run(ic, async () => {

        const start = Date.now();
        // console.log(`calling ${workerNumber} ${functionName} for ${specimenId} at ${new Date(start)}`);
        // const parameters = eval(serializedParameters)

        const p = Promise.resolve(f.call(null, ...resolvedParameters));

        const finishIt = (p: Partial<Pick<InvocationResult, 'output' | 'error'>>) => {
            const end = Date.now();
            const duration = end - start;

            const lines = Array.from(ic.lines).sort((a, b) => a - b);
            const result: InvocationResult = {
                ...p,
                specimenId,
                duration,
                executedBranches: Array.from(executedBranches),
                lines,
                linesInOrder: ic.linesInOrder
            };

            // console.log(`worker ${workerNumber} finishing ${functionName} for ${specimenId}`);

            return result;
        };

        return p.then((output) => {
            return finishIt({ output });
        }).catch((error) => {
            /* 
            TODO: how to differentiate between types of error
            * well-functioning code, e.g. validation
            * likely bug, e.g. attempting to dereference undefined
            * serious error, e.g. stack overflow
            * crash the VM, e.g. out of memory error
            */
            // console.error(`worker ${workerNumber} ${functionName} error for ${specimenId}: ${error} at ${error.stack}`);
            return finishIt({ error: { message: '' + error, stack: error.stack } });
        });
    });
}

// eslint-disable-next-line @typescript-eslint/ban-types
export async function execute(functions: Record<string, Function>) {
    //  running in band so don't act like a worker thread
    if (process.env.MAIN_PROCESS === '1') {
        return;
    }

    if (!parentPort) {
        throw new Error("No parent port");
    }
    const definitelyNotNullParentPortToMakeTypescriptHappy = parentPort;

    //  TODO: run this in a loop to allow reuse of threads OR use a thread pool (benchmark overhead first)
    const { filePath, workerNumber }: WorkerSetup = workerData;

    definitelyNotNullParentPortToMakeTypescriptHappy.on('message', async (message) => {
        await work(functions, workerNumber, message)
            .then((result) => {
                const msg = result;
                definitelyNotNullParentPortToMakeTypescriptHappy.postMessage(msg);
            });
        // const msg = serializeJavascript(result);
    });
}