import { AsyncLocalStorage } from "async_hooks";
import { FunctionDeclaration, Node } from 'typescript';

export interface ExecutionContext {
  executedBranches: Set<string>
}

export const contextStorage = new AsyncLocalStorage<ExecutionContext>();

const getContext = (): ExecutionContext => {
  const context = contextStorage.getStore();
  if (!context) {
    throw new Error('Could not find InstrumentationContext!');
  }
  return context;
};

export function record(branchName: string) {
  const context = getContext();
  context.executedBranches.add(branchName);
}