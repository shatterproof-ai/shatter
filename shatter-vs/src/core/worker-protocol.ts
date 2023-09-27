import { GeneratedParameter } from "./common"

export interface WorkerSetup {
    filePath: string
    workerNumber: number
}

export interface Invocation {
    functionName: string
    serializedParameters: string
    parameters: GeneratedParameter[]
}

export interface InvocationMeta {
    specimenId: string
    launched: number
    invocation: Invocation
}

export interface InvocationResult {
    specimenId: string
    output?: any
    error?: any
    duration: number
    executedBranches: string[]
    lines: number[]
    linesInOrder: number[]
}