import { Specimen, SpecimenId } from '../core/common';
import { cleanUpExtensionState, onPersistedSpecimenLoad, filterClustersForCoverage, findClustersForCoverage, findFunction, findSpecimen, ExtensionState, Specimental } from './common';


const specimentals = (() => {
    const specimens: Specimen[] = [
        { id: 'seed-specimen1', parameters: [], type: 'seed', fileUnderTest: 'dfasdf', functionName: 'baasdfasdf' },
        { id: 'reduction-specimen2', parameters: [], type: 'reduction', parent: 'specimen1', fileUnderTest: 'dfasdf', functionName: 'baasdfasdf' },
        { id: 'custom-specimen3', parameters: [], type: 'hybrid', parents: ['specimen1', 'specimen2'], fileUnderTest: 'dfasdf', functionName: 'baasdfasdf' },
    ];
    const specimentals: Record<SpecimenId, Specimental> = {};
    specimens.forEach(specimen => {
        specimentals[specimen.id] = {
            fileUnderTest: '/dfasdf',
            specimenPath: '/dfasdf/speiejd',
            clusterKey: undefined,
            specimen,
        };
    });

    return specimentals;
})();

const extensionState: Parameters<typeof findSpecimen>[0] & Parameters<typeof findFunction>[0] = {
    fileStates: {
        // eslint-disable-next-line @typescript-eslint/naming-convention
        '/blert/dfasdf': {
            functionStates: {
                'baasdfasdf': {
                    autotest: {
                        clusters: [],
                        instrumentedLines: []
                    },
                    specimens: specimentals,
                },
            },
            functions: [],
        },
    },
    resultClusters: {},
};

describe('cleanUpExtensionState', () => {
    it('if good leave alone', () => {
        const cleaned = cleanUpExtensionState(extensionState);
        expect(cleaned).toEqual(extensionState);
    });
    it('if missing make good', () => {
        const cleaned = cleanUpExtensionState({});
        expect(cleaned.fileStates).toBeTruthy();
    });
});

describe('onPersistedSpecimenLoad', () => {
    it('should return a function that logs the loaded specimen', () => {
        const loadedSpecimen = {
            id: 'specimen-id',
            parameters: [],
            type: 'seed',
        };
        const logLoadedSpecimen = onPersistedSpecimenLoad
    });
});

describe('filterClustersForCoverage', () => {
    it('should return an array of clusters that have at least one uncovered line', () => {
        const clusters = [
            {
                filename: 'file1.js',
                lines: [
                    { line: 1, covered: true },
                    { line: 2, covered: false },
                    { line: 3, covered: true },
                ],
            },
            {
                filename: 'file2.js',
                lines: [
                    { line: 1, covered: true },
                    { line: 2, covered: true },
                    { line: 3, covered: true },
                ],
            },
        ];
        const uncoveredClusters = filterClustersForCoverage(clusters);
        expect(uncoveredClusters).toEqual([
            {
                filename: 'file1.js',
                lines: [
                    { line: 1, covered: true },
                    { line: 2, covered: false },
                    { line: 3, covered: true },
                ],
            },
        ]);
    });
});

describe('findClustersForCoverage', () => {
    it('should return an array of clusters for the given file paths', () => {
        const clusters = [
            {
                filename: 'file1.js',
                lines: [
                    { line: 1, covered: true },
                    { line: 2, covered: false },
                    { line: 3, covered: true },
                ],
            },
            {
                filename: 'file2.js',
                lines: [
                    { line: 1, covered: true },
                    { line: 2, covered: true },
                    { line: 3, covered: true },
                ],
            },
        ];
        const filePaths = ['file1.js'];
        const matchingClusters = findClustersForCoverage(clusters, filePaths);
        expect(matchingClusters).toEqual([
            {
                filename: 'file1.js',
                lines: [
                    { line: 1, covered: true },
                    { line: 2, covered: false },
                    { line: 3, covered: true },
                ],
            },
        ]);
    });
});

describe('findFunction', () => {
    it('should return the function with the given name', () => {
        const functions = [
            { name: 'function1', code: 'function1 code' },
            { name: 'function2', code: 'function2 code' },
            { name: 'function3', code: 'function3 code' },
        ];
        const functionName = 'function2';
        const matchingFunction = findFunction(extensionState, functionName);
        expect(matchingFunction).toEqual({ name: 'function2', code: 'function2 code' });
    });

    it('should return undefined if no function with the given name is found', () => {
        const functions = [
            { name: 'function1', code: 'function1 code' },
            { name: 'function2', code: 'function2 code' },
            { name: 'function3', code: 'function3 code' },
        ];
        const functionName = 'function4';
        const matchingFunction = findFunction(extensionState, functionName);
        expect(matchingFunction).toBeUndefined();
    });
});

describe('findSpecimen', () => {
    it('should return the specimen with the given ID', () => {
        const specimenId = 'reduction-specimen2';
        const matchingSpecimen = findSpecimen(extensionState, specimenId);
        expect(matchingSpecimen).toEqual({ id: 'specimen2', parameters: [], type: 'reduction', parent: 'specimen1' });
    });

    it('should return undefined if no specimen with the given ID is found', () => {
        const specimenId = 'seed-specimen-nope';
        const matchingSpecimen = findSpecimen(extensionState, specimenId);
        expect(matchingSpecimen).toBeUndefined();
    });
});
