import { parentPort, workerData } from 'worker_threads';
import { ExecutionContext, contextStorage } from './recorder';

// eslint-disable-next-line @typescript-eslint/ban-types
export async function execute(functions: Record<string, Function>) {
    if (!parentPort) {
        throw new Error("No parent port");
    }
    const definitelyNotNullParentPortToMakeTypescriptHappy = parentPort;

    //  TODO: run this in a loop to allow reuse of threads OR use a thread pool (benchmark overhead first)
    const { filePath, functionName, parameters, currentWorkerNumber }: { filePath: string, functionName: string, parameters: any, currentWorkerNumber: number } = workerData;
    const executedBranches = new Set<string>();

    const ic: ExecutionContext = {
        executedBranches
    };

    const f = functions[functionName];
    contextStorage.run(ic, async () => {
        const start = Date.now();
        let output: any = undefined;
        let error: any = undefined;
        try {

            output = f.call(null, ...parameters);
            console.log(`${currentWorkerNumber} ${functionName} (${JSON.stringify(parameters)}) => ${JSON.stringify(output)}`);
        } catch (e) {
            error = e;
            console.log(`${currentWorkerNumber} ${functionName} (${JSON.stringify(parameters)}) threw ${e}`);
        } finally {
            const end = Date.now();
            const duration = end - start;

            definitelyNotNullParentPortToMakeTypescriptHappy.postMessage({ output, error, duration, executedBranches: Array.from(executedBranches) });
        }
    });
}