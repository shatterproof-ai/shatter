import { AsyncLocalStorage } from "async_hooks";

export interface ExecutionContext {
  executedBranches: Set<string>
  branchStack: string[]
  lines: Set<number>
}

export const contextStorage = new AsyncLocalStorage<ExecutionContext>();

const getContext = (): ExecutionContext => {
  const context = contextStorage.getStore();
  if (!context) {
    throw new Error('Could not find InstrumentationContext!');
  }
  return context;
};

export function record(branchName: string, meta?: { line?: number, character?: number, filename?: string }) {
  const context = getContext();
  context.executedBranches.add(branchName);
  console.log(`recorded ${branchName} at ${meta?.filename}:${meta?.line}:${meta?.character}`);
}

export function startRecording(branchName: string) {
  const context = getContext();
  context.executedBranches.add(branchName);
  context.branchStack.push(branchName);
}

export function stopRecording(branchName: string) {
  const context = getContext();

  const peek = context.branchStack?.[context.branchStack.length - 1];
  if (peek === branchName) {
    context.branchStack.pop();
    return;
  }

  //  TODO: well this is bad
}

export function recordLine(line: number) {
  const context = getContext();
  context.lines.add(line);
}