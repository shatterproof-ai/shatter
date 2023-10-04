import { GeneratedParameter, SpecimenId } from "./common";

export interface WorkerSetup {
    filePath: string
    workerNumber: number
}

export interface Invocation {
    functionName: string
    serializedParameterValues: string
}

export interface InvocationMeta {
    specimenId: string
    launched: number
    invocation: Invocation
    generatedParameters: GeneratedParameter[]
}

export interface InvocationResult {
    specimenId: SpecimenId
    output?: any
    error?: { message: string, stack: any }
    duration: number
    executedBranches: string[]
    lines: number[]
    linesInOrder: number[]
}