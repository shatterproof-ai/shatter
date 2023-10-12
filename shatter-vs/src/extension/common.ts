import { AbsolutePath, RelativePath, Specimen, SpecimenId } from "../core/common";
import { AutotestResults } from "../core/shatter";
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
    //	this overlaps some with specimens, but it doesn't load the contents	
    expected?: Record<SpecimenId, Expected>;
};

//	this exists primarily for the situation where the ExtensionState that was
//	persisted has a different structure than what the code uses now
export function cleanUpExtensionState(initial: Partial<ExtensionState>) {
    const fullExtensionState: ExtensionState = {
        fileStates: {},
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

export function onPersistedSpecimenLoad(absolutist: (r: RelativePath) => AbsolutePath, extensionState: ExtensionState, specimen: Specimen, maybeSpecimenId: string, absoluteSpecimenFilepath: AbsolutePath | undefined) {
    const absoluteSourceFilepath = absolutist(specimen.fileUnderTest);
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
