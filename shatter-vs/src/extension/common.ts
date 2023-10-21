import { AbsolutePath, RelativePath, Specimen, SpecimenId } from "../core/common";
import { AutotestResults, ResultCluster } from "../core/shatter";
import { Outcome, TestRun, isOutcome } from "../core/supervisor";
import { FunctionMeta } from "../core/transform";

export interface Specimental {
    fileUnderTest: AbsolutePath,
    specimenPath?: AbsolutePath,			//	empty if not persisted
    clusterKey?: string,	//	empty if never run
    specimen: Specimen,
}

export interface Expected {
    expectedPath?: AbsolutePath,	//	empty if not persisted
    result: TestRun,
}

export type FunctionState = {
    autotest: AutotestResults;
    specimens: Record<string, Specimental>;	//	Record because Map is not serializable
};

export type FileState = {
    functions: FunctionMeta[];
    functionStates: Record<string, FunctionState>;	//	Record because Map is not serializable
};

export type CoverageSelection = 'all'
    | 'missed'
    | Outcome
    | { clusterKey: string };

export function isCoverageSelection(s:any): s is CoverageSelection {
    return (typeof s === 'object' && typeof s.clusterKey === 'string')
    || s === 'missed'
    || s === 'all'
    || isOutcome(s);
}

export interface ExtensionState {
    runningTestFunction?: string;
    fileStates: Record<AbsolutePath, FileState>;	//	Record because Map is not serializable
    //  these are also in fileStates[...].functionStates[...].autotest.clusters but we want random access
    resultClusters: Record<SpecimenId, ResultCluster>;
    //	this overlaps some with specimens
    expected?: Record<SpecimenId, Expected>;
};

//	this exists primarily for the situation where the ExtensionState that was
//	persisted has a different structure than what the code uses now
export function cleanUpExtensionState(initial: Partial<ExtensionState>) {
    const fullExtensionState: ExtensionState = {
        fileStates: {},
        resultClusters: {},
        ...initial,
    };

    if (!fullExtensionState.fileStates) {
        fullExtensionState.fileStates = {};
    }

    for (const [filename, fileState] of Object.entries(fullExtensionState.fileStates)) {
        if (!fileState.functions) {
            fileState.functions = [];
        }
        if (!fileState.functionStates) {
            fileState.functionStates = {};
        }
        for (const [functionName, functionState] of Object.entries(fileState.functionStates)) {
            if (!functionState.specimens) {
                //	at least once there was a failed serialization and the specimens property wasn't present
                functionState.specimens = {};
            }
        }
    }

    return fullExtensionState;
}

export function onPersistedSpecimenLoad(absoluteSourceFilepath: AbsolutePath, extensionState: ExtensionState, specimen: Specimen, maybeSpecimenId: string, absoluteSpecimenFilepath: AbsolutePath | undefined) {
    if (!extensionState.fileStates[absoluteSourceFilepath]) {
        extensionState.fileStates[absoluteSourceFilepath] = {
            functions: [],
            functionStates: {},
        };
    }

    const fileState = extensionState.fileStates[absoluteSourceFilepath];
    if (!fileState.functionStates[specimen.functionName]) {
        fileState.functionStates[specimen.functionName] = {
            autotest: {
                clusters: [],
                instrumentedLines: [],
            },
            specimens: {},
        };
    }

    const functionState = fileState.functionStates[specimen.functionName];
    const existing = functionState.specimens[maybeSpecimenId];
    if (existing) {
        console.log(`Unexpectedly (?) found existing specimen ${maybeSpecimenId} for ${specimen.functionName} in ${absoluteSourceFilepath}`);
    }

    functionState.specimens[specimen.id] = {
        fileUnderTest: absoluteSourceFilepath,
        specimenPath: absoluteSpecimenFilepath,
        clusterKey: undefined,
        specimen,
    };
}

export function filterClustersForCoverage(coverage: Exclude<CoverageSelection, 'missed'> | undefined, clusters?: ResultCluster[]): ResultCluster[] {
    if (clusters === undefined) {
        return [];
    }

    if (coverage === undefined) {
        return clusters;
    }

    if (typeof coverage === 'string') {
        if (coverage === 'all') {
            return clusters;
        }

        if (isOutcome(coverage)) {
            return clusters.filter(c => c.outcome === coverage);
        }

        return [];
    }

    return clusters.filter(c => coverage.clusterKey === c.key);
}

export const findClustersForCoverage = (extensionState: ExtensionState, coverage: Exclude<CoverageSelection, 'missed'>): ResultCluster[] => {
    const allMatches: ResultCluster[] = [];
    for (const fileState of Object.values(extensionState.fileStates)) {
        for (const functionState of Object.values(fileState.functionStates)) {
            const functionMatches = filterClustersForCoverage(coverage, functionState.autotest.clusters);
            allMatches.push(...functionMatches);
        }
    }
    return allMatches;
};

export const findFunction = (extensionState: ExtensionState, functionName: string): [FunctionMeta, FunctionState] | undefined => {
    for (const fileState of Object.values(extensionState.fileStates)) {
        if (fileState.functionStates[functionName]) {
            for (const functionMeta of fileState.functions) {
                if (functionMeta.name === functionName) {
                    return [functionMeta, fileState.functionStates[functionName]];
                }
            }
        }
    }
};

export const findSpecimen = (extensionState: Pick<ExtensionState, 'fileStates'>, specimenId: SpecimenId): Specimental | undefined => {
    for (const fileState of Object.values(extensionState.fileStates)) {
        for (const functionState of Object.values(fileState.functionStates)) {
            const maybeSpecimental = functionState.specimens[specimenId];
            if (maybeSpecimental) {
                return maybeSpecimental;
            }
        }
    }
};
