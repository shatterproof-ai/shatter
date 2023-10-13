import { capitalize, isEqual, result } from "lodash";
import { AbsolutePath, Specimen, SpecimenId, extractGeneratedParameterValue, isSpecimenId, resolveGeneratedParameterValue } from "../core/common";
import { ResultCluster } from "../core/shatter";
import { Outcome, TestRun, isOutcome } from "../core/supervisor";
import { FunctionMeta, findFunctions } from "../core/transform";
import { CoverageSelection, Expected, ExtensionState, FileState, FunctionState, Specimental, isCoverageSelection } from "./common";

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
    getSelected(): readonly CommonDisplayNode[];
}

export interface DisplayProviders {
    functionsListProvider: DisplayProvider;
    clustersListProvider: DisplayProvider;
    testCaseListProvider: DisplayProvider;
    testCaseDetailProvider: DisplayProvider;
}

export interface SelectedElements {
    selectedFile?: {
        filename: AbsolutePath;
        state: FileState;
    }

    selectedFunction?: {
        name: string;
        state: FunctionState;
    }

    coverage?: {
        selectedCoverage?: CoverageSelection;
        clusters?: ResultCluster[];
    }

    specimental?: Specimental;
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

type TestStatus = 'pass' | 'fail' | 'running' | 'unknown';
function speciminode(expectedResults: Record<SpecimenId, Expected> | undefined, functionState: FunctionState | undefined, specimental: Specimental) {
    let actual: TestRun | undefined = undefined;
    let cluster: ResultCluster | undefined = undefined;
    for (const c of functionState?.autotest.clusters ?? []) {
        actual = c.results.find(r => r.specimenId === specimental.specimen.id);
        if (actual) {
            cluster = c;
            break;
        }
    }

    const isEdgy = cluster?.leasts.findIndex(l => l.id === specimental.specimen.id) !== -1
        || cluster?.mosts.findIndex(l => l.id === specimental.specimen.id) !== -1;

    const expected = expectedResults?.[specimental.specimen.id];

    const contextPieces: string[] = [];

    if (specimental?.specimen?.id.startsWith('custom')) {
        contextPieces.push('custom');
    } else {
        contextPieces.push('autotest');
    }

    if (specimental?.specimenPath) {
        contextPieces.push('persistent');
    }

    if (isEdgy) {
        contextPieces.push('edge');
    }

    const pinState = specimental?.specimenPath ? 'pinned' : 'unpinned';
    contextPieces.push(pinState);

    //  TODO: this is the most primitive acceptable approach
    const testStatus: TestStatus | undefined = (() => {
        if (actual === undefined) {
            return undefined;
        }

        if (expected === undefined) {
            return 'unknown';
        }

        if (expected.result.outcome !== actual.outcome) {
            return 'fail';
        }

        if (expected.result.outcome === 'completed') {
            if (!isEqual(expected.result.result?.returnValue, actual.result?.returnValue)) {
                return 'fail';
            }
        }

        return 'pass';
    })();

    if (testStatus) {
        contextPieces.push(testStatus);
    };

    const node: CommonDisplayNode = {
        label: specimental.specimen.id,
        key: specimental.specimen.id,
        contextValue: 'testcase',
        state: testStatus,
    };

    return node;
}

export const refresh = (selectedElements: SelectedElements, extensionState: ExtensionState, providers: DisplayProviders, highlighters: Record<AbsolutePath, Highlighter>) => {
    const { functionsListProvider, clustersListProvider, testCaseListProvider, testCaseDetailProvider } = providers;

    const fileState = selectedElements.selectedFile?.state;
    const functionState = selectedElements.selectedFunction?.state;
    const functionMeta = fileState?.functions.find(f => f.name === selectedElements.selectedFunction?.name);
    const specimental = selectedElements.specimental;

    console.log(`${Object.keys(extensionState.fileStates).length} file states; ${functionState ? functionState.autotest.clusters.length : 0} clusters; ${specimental ? specimental.specimen.id : 'no specimen'}`);

    if (!fileState) {
        functionsListProvider.refresh([]);
        clustersListProvider.refresh([]);
        testCaseListProvider.refresh([]);
        testCaseDetailProvider.refresh([]);
        return;
    }

    const persistentSpecimensByFunction = new Map<string, Specimental[]>();
    if (functionState?.specimens) {
        for (const specimental of Object.values(functionState.specimens)) {
            if (specimental.specimenPath) {
                const specimens = persistentSpecimensByFunction.get(specimental.specimen.functionName) ?? [];
                specimens.push(specimental);
                persistentSpecimensByFunction.set(specimental.specimen.functionName, specimens);
            }
        }
    }

    const nodes: CommonDisplayNode[] = fileState.functions.map((f) => {
        const runningTest = extensionState.runningTestFunction === f.name;
        const runningTestLabel = runningTest ? " - testing now" : "";
        const children: undefined | CommonDisplayNode[] = persistentSpecimensByFunction.get(f.name)
            ?.map(s => speciminode(extensionState.expected, functionState, s));
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

    if (selectedElements.selectedFunction) {
        functionsListProvider.select(selectedElements.selectedFunction.name);
    }

    const activeCoverage = selectedElements.coverage?.selectedCoverage;

    const results = functionState.autotest;
    const allClusters = results.clusters;
    const selectedClusters = activeCoverage === 'missed'
        ? []
        : filterClustersForCoverage(activeCoverage, allClusters);

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
                    const count = countByOutcome[outcome as Outcome] ?? 0;
                    const plural = count === 1 ? '' : 's';
                    if (outcome === 'timeout' || outcome === 'failed') {
                        return `${baseLabel} (${count} test case${plural})`;
                    }
                    if (functionInstrumentedLines.size === 0) {
                        return `${baseLabel} (${count} test case${plural})`;
                    }
                    const coverage = linesByOutcome[outcome as Outcome].size / functionInstrumentedLines.size;
                    return `${baseLabel} - ${formatter.format(coverage)} coverage (${count} test case${plural})`;
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

        const highlighter = highlighters[selectedElements.selectedFile?.filename!];
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

    const testCaseListNodes: CommonDisplayNode[] = selectedClusters
        .flatMap(c => c.results.map((result, i): CommonDisplayNode => {
            const specimental = functionState.specimens[result.specimenId];
            const node = speciminode(extensionState.expected, functionState, specimental);
            return node;
        }));
    testCaseListProvider.refresh(testCaseListNodes);

    if (!specimental) {
        return;
    }

    const testRun = (() => {
        for (const cluster of selectedClusters) {
            const result = cluster.results.find(c => c.specimenId === specimental.specimen.id);
            if (result) {
                return result;
            }
        }
    })();

    if (!testRun) {
        return;
    }

    const resolvedParameters = specimental.specimen.parameters.map(extractGeneratedParameterValue);
    const parametersValueNodes = valueToNode(resolvedParameters, 3);
    const parametersNode: CommonDisplayNode = {
        label: 'Parameters',
        children: parametersValueNodes,
    };

    const metadataNode = {
        label: `Result: ${capitalize(testRun.outcome)} in ${testRun.result?.duration ?? testRun.runnerDuration}ms`
    };

    const testCaseNodes: CommonDisplayNode[] = [
        metadataNode,
        parametersNode,
    ];

    if (testRun.result?.error) {
        const unstrungError = testRun.result.error;
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
    } else if (testRun.result?.returnValue) {
        const outputValuesNodes = valueToNode(testRun.result.returnValue, 3);
        const outputNode: CommonDisplayNode = {
            label: 'Return value',
            children: outputValuesNodes,
        };
        testCaseNodes.push(outputNode);
    } else {
        const noResultsNode: CommonDisplayNode = {
            label: `No results`,
        };
        testCaseNodes.push(noResultsNode);
    }

    testCaseDetailProvider.refresh(testCaseNodes);
};