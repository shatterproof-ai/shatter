import { capitalize } from "lodash";
import { AbsolutePath, Specimen, SpecimenId, extractGeneratedParameterValue, resolveGeneratedParameterValue } from "../core/common";
import { ResultCluster } from "../core/shatter";
import { Outcome, isOutcome } from "../core/supervisor";
import { FunctionMeta, findFunctions } from "../core/transform";
import { CoverageSelection, ExtensionState, FunctionState, Specimental, getActiveStates } from "./common";

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
    select(key: string): void;
}

export interface DisplayProviders {
    functionsListProvider: DisplayProvider;
    clustersListProvider: DisplayProvider;
    testCaseListProvider: DisplayProvider;
    testCaseDetailProvider: DisplayProvider;
}

export function findNode(nodes: CommonDisplayNode[], key: string): CommonDisplayNode | undefined {
    for (const node of nodes) {
        if (node.key === key) {
            return node;
        }
        if (node.children) {
            const childNode = findNode(node.children, key);
            if (childNode) {
                return childNode;
            }
        }
    }
    return undefined;
}

function valueToNode(o: any, depth = 0): CommonDisplayNode[] {
    if (depth === 0) {
        return [{
            label: "...",
        }];
    }

    if (o === null) {
        return [{
            label: `null`,
        }];
    }
    if (o === undefined) {
        return [{
            label: `undefined`,
        }];
    }
    if (typeof o === 'object') {
        if (Array.isArray(o)) {
            const children: CommonDisplayNode[] = [];
            for (let i = 0; i < o.length; i++) {
                const elementNodes = valueToNode(o[i], depth - 1);
                children.push({
                    label: `[${i}]`,
                    children: elementNodes,
                });
            }

            return children;
        }

        const children: CommonDisplayNode[] = [];
        for (const [k, v] of Object.entries(o)) {
            const elementNode = valueToNode(v, depth - 1);
            children.push({
                label: k,
                children: elementNode,
            });
        }

        return children;
    }

    return [{
        label: JSON.stringify(o),
    }];
}

export function filterClustersForCoverage(coverage: CoverageSelection | undefined, clusters?: ResultCluster[]): ResultCluster[] {
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

export const findClustersForCoverage = (extensionState: ExtensionState, coverage: Exclude<CoverageSelection, 'missing'>): ResultCluster[] => {
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

export const findSpecimen = (extensionState: ExtensionState, specimenId: SpecimenId): Specimental | undefined => {
    for (const fileState of Object.values(extensionState.fileStates)) {
        for (const functionState of Object.values(fileState.functionStates)) {
            const maybeSpecimental = functionState.specimens[specimenId];
            if (maybeSpecimental) {
                return maybeSpecimental;
            }
        }
    }
};

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

    const persistentSpecimensByFunction = new Map<string, Specimen[]>();
    if (functionState?.specimens) {
        for (const specimental of Object.values(functionState.specimens)) {
            if (specimental.specimenPath) {
                const specimens = persistentSpecimensByFunction.get(specimental.specimen.functionName) ?? [];
                specimens.push(specimental.specimen);
                persistentSpecimensByFunction.set(specimental.specimen.functionName, specimens);
            }
        }
    }

    const nodes: CommonDisplayNode[] = fileState.functions.map((f) => {
        const runningTest = extensionState.runningTestFunction === f.name;
        const runningTestLabel = runningTest ? " - testing now" : "";
        const children: undefined | CommonDisplayNode[] = persistentSpecimensByFunction.get(f.name)
            ?.map((specimen) => {
                const node: CommonDisplayNode = {
                    label: specimen.id,
                    key: specimen.id,
                    contextValue: 'testcase',
                };
                return node;
            });
        return {
            label: `${f.name}${runningTestLabel}` || "",
            key: f.name || "",
            contextValue: 'function',
            children,
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

    if (extensionState.activeFunction) {
        functionsListProvider.select(extensionState.activeFunction);
    }

    const activeCoverage = extensionState.activeCoverage;

    const results = functionState.autotest;
    const allClusters = results.clusters;
    const selectedClusters: ResultCluster[] = filterClustersForCoverage(activeCoverage, allClusters);

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
        allClusters.forEach((cluster) => {
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
        if (activeCoverage) {
            if (typeof activeCoverage !== 'string') {
                //  TODO: what if clusterKeys.length > 1?
                clustersListProvider.select(activeCoverage.clusterKey);
            } else if (isOutcome(activeCoverage)) {
                functionsListProvider.select(activeCoverage);
            }
        }

        if (activeCoverage === 'missed') {
            function* missedLinerator() {
                const allCovered = new Set(results.clusters.flatMap((cluster) => cluster.lines));
                const uncovered = Array.from(functionInstrumentedLines)
                    .filter((line) => !allCovered.has(line))
                    .sort((a, b) => a - b);
                for (const line of uncovered ?? []) {
                    yield line;
                }

            }

            highlighter('missed', missedLinerator);
        } else {
            function* selectedCoveredLinerator() {
                const selectedCovered = new Set(selectedClusters.flatMap((cluster) => cluster.lines));
                const lines = Array.from(selectedCovered).sort((a, b) => a - b);
                for (const line of lines ?? []) {
                    yield line;
                }
            }

            highlighter('covered', selectedCoveredLinerator);
        }
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

    const edgeCaseSpecimens = new Set<SpecimenId>();
    for (const cluster of allClusters) {
        for (const specimen of cluster.leasts) {
            edgeCaseSpecimens.add(specimen.id);
        }
        for (const specimen of cluster.mosts) {
            edgeCaseSpecimens.add(specimen.id);
        }
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

            if (edgeCaseSpecimens.has(specimental.specimen.id)) {
                contextPieces.push('edge');
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

    const resolvedParameters = specimental.specimen.parameters.map(extractGeneratedParameterValue);
    const parametersValueNodes = valueToNode(resolvedParameters, 3);
    const parametersNode: CommonDisplayNode = {
        label: 'Parameters',
        children: parametersValueNodes,
    };

    const metadataNode = {
        label: `Result: ${capitalize(result.outcome)} in ${result.duration}ms`
    };

    const testCaseNodes: CommonDisplayNode[] = [
        metadataNode,
        parametersNode,
    ];

    if (result.output) {
        const outputValuesNodes = valueToNode(result.output, 3);
        const outputNode: CommonDisplayNode = {
            label: 'Return value',
            children: outputValuesNodes,
        };
        testCaseNodes.push(outputNode);
    } else if (result.error) {
        const unstrungError = JSON.parse(result.error);
        const errorNode: CommonDisplayNode = {
            label: unstrungError.message,
        };
        const stack: string | undefined = unstrungError.stack;
        if (stack) {
            errorNode.children = stack.split('\n')
                .filter((line: string, i: number) => i !== 0 || line !== unstrungError.message)
                .map((frame: any): CommonDisplayNode => ({ label: frame }));
        }
        testCaseNodes.push(errorNode);
    } else {
        const noResultsNode: CommonDisplayNode = {
            label: `No results`,
        };
        testCaseNodes.push(noResultsNode);
    }

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
    coverage: CoverageSelection | undefined) => {
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
        extensionState.fileStates[absoluteSourceFilename].functionStates = Object.fromEntries(
            functions.map((f):[string, FunctionState] => [f.name, { specimens: {}, autotest: {clusters: [], instrumentedLines:[]} }])
        );
    } else {
        extensionState.fileStates[absoluteSourceFilename] = {
            functionStates: {},
            functions,
        };
    }

    refresh(extensionState, providers, highlighter);
}
