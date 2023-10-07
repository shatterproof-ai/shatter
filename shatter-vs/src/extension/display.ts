import { filter } from "lodash";
import { AbsolutePath } from "../core/common";
import { ResultCluster } from "../core/shatter";
import { Outcome, Outcomes, isOutcome } from "../core/supervisor";
import { findFunctions } from "../core/transform";
import { CoverageSelection, ExtensionState, getActiveStates } from "./common";

export type Highlighter = (decoration: 'covered' | 'missed', liner: () => Generator<number, void, unknown>) => void;

export interface CommonDisplayNode {
    label: string;
    children?: CommonDisplayNode[];
    key?: string,
    state?: string,
    contextValue?: string,
}

export interface DisplayProvider {
    refresh(nodes: CommonDisplayNode[]): void;
}

export interface DisplayProviders {
    functionsListProvider: DisplayProvider;
    clustersListProvider: DisplayProvider;
    testCaseListProvider: DisplayProvider;
    testCaseDetailProvider: DisplayProvider;
}

function visit(k: string | number, o: any, depth = 0): CommonDisplayNode {
    if (depth === 0) {
        return {
            label: "...",
        };
    }

    const key = typeof k === 'number' ? `[${k}]` : `"${k}"`;
    if (o === null) {
        return {
            label: `${key}: null`,
        };
    }
    if (o === undefined) {
        return {
            label: `${key}: undefined`,
        };
    }
    if (typeof o === 'object') {
        if (Array.isArray(o)) {
            return {
                label: key,
                children: o.map((v, i) => visit(i, v, depth - 1)),
            };
        }
        const keys = Object.keys(o);
        const children = keys.map((k) => visit(k, o[k], depth - 1));
        return {
            label: key,
            children,
        };
    }

    return {
        label: `${key}: ${JSON.stringify(o)}`,
    };
}

function filterClustersForCoverage(coverage: CoverageSelection | undefined, clusters?: ResultCluster[]): ResultCluster[] {
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

    return clusters.filter(c => coverage.clusterKeys.includes(c.key));
}

export const refresh = (extensionState: ExtensionState, providers: DisplayProviders, highlighter: Highlighter) => {
    const { functionsListProvider, clustersListProvider, testCaseListProvider, testCaseDetailProvider } = providers;

    const { fileState, functionState, functionMeta, specimental } = getActiveStates(extensionState);

    console.log(`${Object.keys(extensionState.fileStates).length} file states; ${functionState ? functionState.autotest.clusters.length : 0} clusters; ${specimental ? specimental.specimen.id : 'no specimen'}`);

    if (!fileState) {
        functionsListProvider.refresh([]);
        clustersListProvider.refresh([]);
        testCaseListProvider.refresh([]);
        testCaseDetailProvider.refresh([]);
        return;
    }

    const nodes: CommonDisplayNode[] = fileState.functions.map((f) => {
        const runningTest = extensionState.runningAutotestFunction === f.name;
        const runningTestLabel = runningTest ? " - testing now" : "";
        return {
            label: `${f.name}${runningTestLabel}` || "",
            key: f.name || "",
        };
    });

    functionsListProvider.refresh(nodes);

    // if (!functionState) {
    // console.log(`nonono results for filename "${filename}" and function "${extensionState.activeFunction}" - ${JSON.stringify(fileState.functionStates)}`)
    // return;
    // };

    if (!functionState || !functionMeta) {
        clustersListProvider.refresh([]);
        testCaseListProvider.refresh([]);
        testCaseDetailProvider.refresh([]);
        return;
    }

    const activeCoverage = extensionState.activeCoverage;

    const results = functionState.autotest;
    const selectedClusters: ResultCluster[] = filterClustersForCoverage(activeCoverage, results.clusters);

    if (results) {
        const nodesByOutcome: Record<Outcome, CommonDisplayNode[]> = {
            completed: [],
            error: [],
            timeout: [],
            failed: []
        };

        const countByOutcome: Record<Outcome, number> = {
            completed: 0,
            error: 0,
            timeout: 0,
            failed: 0
        };

        const linesByOutcome: Record<Outcome, Set<number>> = {
            error: new Set(),
            completed: new Set(),
            timeout: new Set(),
            failed: new Set(),
        };

        const functionInstrumentedLines = new Set<number>();
        for (let line = functionMeta.startLine; line <= functionMeta.endLine; line++) {
            if (functionState.autotest.instrumentedLines.includes(line)) {
                functionInstrumentedLines.add(line);
            }
        }

        const formatter = Intl.NumberFormat("en-US", { style: "percent" });
        //	TODO: sort by coverage
        selectedClusters.forEach((cluster) => {
            const key = cluster.key.substring(0, 6);
            countByOutcome[cluster.outcome] += cluster.results.length;
            cluster.lines.forEach(line => linesByOutcome[cluster.outcome].add(line));

            const clusterStatus = functionInstrumentedLines.size > 0
                ? `${formatter.format(cluster.lines.length / functionInstrumentedLines.size)} coverage (${cluster.results.length} test cases)`
                : "Nothing yet";

            nodesByOutcome[cluster.outcome].push({
                //	TODO: skip coverage for timeouts and failures
                label: `${key} - ${clusterStatus}`,
                key: `cluster://${cluster.key}`,
            });
        });

        const capitalize = (s: string) => {
            return s.charAt(0).toUpperCase() + s.slice(1);
        };

        const clusterNodes: CommonDisplayNode[] = Object.entries(nodesByOutcome)
            .map(([outcome, nodes]) => {
                const baseLabel = capitalize(outcome);
                const label = (() => {
                    if (outcome === 'timeout' || outcome === 'failed') {
                        return baseLabel;
                    }
                    if (functionInstrumentedLines.size === 0) {
                        return baseLabel;
                    }
                    const coverage = linesByOutcome[outcome as Outcome].size / functionInstrumentedLines.size;
                    return `${baseLabel} - ${formatter.format(coverage)} coverage (${countByOutcome[outcome as Outcome] ?? 0} test case(s))`;
                })();

                return {
                    label,
                    children: nodes,
                    key: outcome,
                };
            });

        UNCOVERED: {
            const allCoveredLines = new Set<number>();
            Object.values(linesByOutcome).forEach((lines) => {
                lines.forEach((line) => allCoveredLines.add(line));
            });
            const totalCoverageFraction = allCoveredLines.size / functionInstrumentedLines.size;
            const uncoveredFraction = 1 - totalCoverageFraction;

            const label = functionInstrumentedLines.size === 0
                ? `Not covered `
                : `Not covered ${formatter.format(uncoveredFraction)} (${functionInstrumentedLines.size - allCoveredLines.size} lines)`;

            clusterNodes.push({
                label,
                key: "missed://",
            });
        }

        clustersListProvider.refresh(clusterNodes);

        function* linerator() {
            const covered = new Set(selectedClusters.flatMap((cluster) => cluster.lines));
            const lines = (() => {
                if (activeCoverage === 'missed') {
                    const uncovered = Array.from(functionInstrumentedLines)
                        .filter((line) => !covered.has(line))
                        .sort((a, b) => a - b);
                    return uncovered;
                }
                return Array.from(covered).sort((a, b) => a - b);
            })();

            for (const line of lines ?? []) {
                yield line;
            }
        }
        highlighter(activeCoverage === 'missed' ? 'missed' : 'covered', linerator);
    }

    const shortString = (a: any) => {
        if (a === null) {
            return 'null';
        }
        if (a === undefined) {
            return 'undefined';
        }
        const s = typeof a === 'string' ? a : JSON.stringify(a);
        const maxLength = 40;

        const strung = (s.length > maxLength)
            ? s.substring(0, maxLength - 3) + '...'
            : s;
        return strung;
    };

    if (activeCoverage === 'missed') {
        testCaseListProvider.refresh([]);
        testCaseDetailProvider.refresh([]);
        return;
    }

    const testCaseListNodes: CommonDisplayNode[] = selectedClusters
        .flatMap(c => c.results.map((result, i): CommonDisplayNode => {
            const specimental = functionState.specimens[result.specimenId];

            const contextPieces: string[] = [];

            if (specimental?.specimen?.id.startsWith('custom')) {
                contextPieces.push('custom');
            } else {
                contextPieces.push('autotest');
            }

            if (specimental?.specimenPath) {
                contextPieces.push('persistent');
            }

            const state = specimental?.specimenPath ? 'pinned' : 'unpinned';
            const parametersNode = {
                label: shortString(result.serializedParameterValues),
                key: result.specimenId,
                state,
                contextValue: contextPieces.join(','),
            };
            return parametersNode;
        }));
    testCaseListProvider.refresh(testCaseListNodes);

    if (!specimental) {
        return;
    }

    const result = (() => {
        for (const cluster of selectedClusters) {
            const result = cluster.results.find(c => c.specimenId === specimental.specimen.id);
            if (result) {
                return result;
            }
        }
    })();

    if (!result) {
        return;
    }

    //	TODO: make this cleaner, ideally like JSON.stringify(...)
    const metadataNode = {
        label: `Duration ${result.duration}ms`
    };
    const resultNode = visit('Result', result.output ?? result.error, 3);

    if (!specimental.specimen) {
        console.error(`Unable to find specimen ${result.specimenId}`);
        return;
    }

    const parametersNode = visit('Parameters', specimental.specimen.parameters, 3);
    const testCaseNodes: CommonDisplayNode[] = [
        metadataNode,
        parametersNode,
        resultNode,
    ];

    testCaseDetailProvider.refresh(testCaseNodes);
};

export const doSelectFunction = (highlighter: Highlighter, extensionState: ExtensionState, providers: DisplayProviders, functionName: string) => {
    if (!extensionState.activeFile) {
        //	TODO: shouldn't happen
        return;
    }
    const filename = extensionState.activeFile;
    const filestate = extensionState.fileStates[filename];
    if (!filestate) {
        //	TODO: shouldn't happen; TODO: can regenerate
        return;
    }

    const selectedFunction = filestate.functions.find((f) => f.name === functionName);
    if (selectedFunction) {
        extensionState.activeFunction = functionName;
    } else {
        extensionState.activeCoverage = undefined;
        extensionState.activeFunction = undefined;
    }
    refresh(extensionState, providers, highlighter);
};

export const doSelectCluster = (highlighter: Highlighter, extensionState: ExtensionState, providers: DisplayProviders,
    coverage: CoverageSelection|undefined) => {
    if (!extensionState.activeFile) {
        //	TODO: shouldn't happen
        return;
    }
    const filename = extensionState.activeFile;
    const filestate = extensionState.fileStates[filename];
    if (!filestate) {
        //	TODO: shouldn't happen
        return;
    }

    if (!extensionState.activeFunction) {
        return;
    }

    const functions = findFunctions(filename);

    const selectedFunction = functions.find((f) => f.name === extensionState.activeFunction);
    if (!selectedFunction) {
        //	TODO: shouldn't happen
        return;
    }

    const functionState = filestate.functionStates[extensionState.activeFunction];
    if (!functionState) {
        //	TODO: shouldn't happen
        return;
    }

    extensionState.activeCoverage = coverage;
    refresh(extensionState, providers, highlighter);
};

export const doSelectTestCase = (highlighter: Highlighter, extensionState: ExtensionState, providers: DisplayProviders,
    specimenId: string) => {
    if (!extensionState.activeFile) {
        return;
    }

    const filename = extensionState.activeFile;
    const filestate = extensionState.fileStates[filename];
    if (!filestate) {
        return;
    }

    if (!extensionState.activeFunction) {
        return;
    }

    const functions = findFunctions(filename);

    const selectedFunction = functions.find((f) => f.name === extensionState.activeFunction);
    if (!selectedFunction) {
        //	TODO: shouldn't happen
        return;
    }

    const functionState = filestate.functionStates[extensionState.activeFunction];
    if (!functionState) {
        //	TODO: shouldn't happen
        return;
    }

    extensionState.activeSpecimenId = specimenId;
    refresh(extensionState, providers, highlighter);
};

export function doSelectFile(highlighter: Highlighter, extensionState: ExtensionState, absoluteSourceFilename: AbsolutePath, providers: DisplayProviders) {
    if (extensionState.activeFile !== absoluteSourceFilename) {
        extensionState.activeFile = absoluteSourceFilename;
        extensionState.activeFunction = undefined;
        extensionState.activeCoverage = undefined;
        extensionState.activeSpecimenId = undefined;
    }

    const functions = findFunctions(absoluteSourceFilename);
    /*
    Typescript didn't like this spread
        extensionState.fileStates[filename] = {
            functionStates: {},
            ...extensionState.fileStates[filename],
            functions,
        };

     */
    if (extensionState.fileStates[absoluteSourceFilename]) {
        extensionState.fileStates[absoluteSourceFilename].functions = functions;
    } else {
        extensionState.fileStates[absoluteSourceFilename] = {
            functionStates: {},
            functions,
        };
    }

    refresh(extensionState, providers, highlighter);
}
