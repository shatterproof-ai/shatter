import { AbsolutePath, Specimen, SpecimenId, joinAbsolute } from '../core/common';
import { ExtensionState, Specimental, cleanUpExtensionState, filterClustersForCoverage, findClustersForCoverage, findFunction, findSpecimen, onPersistedSpecimenLoad } from './common';


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

import { writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { ResultCluster } from '../core/shatter';

describe('onPersistedSpecimenLoad', () => {
    it('should return a function that logs the loaded specimen', () => {
        const s:Specimen = {
            id: 'seed-specimen-id',
            parameters: [],
            type: 'seed',
            fileUnderTest: 'file-under-test',
            functionName: 'function-name',
        };

        const filePath = joinAbsolute(tmpdir() as AbsolutePath, 'loadedSpecimen.json');
        writeFileSync(filePath, JSON.stringify(s));

        const extensionState:ExtensionState = {
            fileStates: {},
            resultClusters: {},
        };
        onPersistedSpecimenLoad('/doesnt/amtter', extensionState, s, s.id, filePath);

        expect(extensionState.fileStates).toEqual({
            '/doesnt/amtter': {
                functionStates: {
                    'function-name': {
                        autotest: {
                            clusters: [],
                            instrumentedLines: [],
                        },
                        specimens: {
                            'seed-specimen-id': {
                                fileUnderTest: '/doesnt/amtter',
                                specimenPath: filePath,
                                clusterKey: undefined,
                                specimen: s,
                            },
                        },
                    },
                },
                functions: [],
            },
        });
    });
});
const clusters:ResultCluster[] = [
    {
        key: "akakakaka",
        file: '/tmp/file1.js',
        functionName: "flerp",
        lines: [],
        linesInOrder: [],
        specimens: [],
        outcome: 'completed',
        results: [],
        leasts: [],
        mosts: [],
        totalTime: 535,
        distancesToClusters: [],
    },
];

describe('filterClustersForCoverage', () => {
    it('should return an array of clusters that have at least one uncovered line', () => {
        const uncoveredClusters = filterClustersForCoverage('completed', clusters);
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
        const filePaths = ['file1.js'];
        const matchingClusters = findClustersForCoverage(extensionState, {clusterKey:'aaaa'});
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
