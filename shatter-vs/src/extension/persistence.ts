import * as fs from 'fs'; //TODO: use VSCode fs
import * as path from 'path';
import { join } from 'path';
import { AbsolutePath, RelativePath, Specimen, SpecimenId, joinAbsolute } from '../core/common';
import { TestRun } from '../core/supervisor';
import { Expected, ExtensionState, Specimental, findSpecimen } from './common';

const SPECIMENS_SUBDIR = 'specimens';
const EXPECTED_SUBDIR = 'expected';

function loadExpectedResult(filepath: AbsolutePath) {
    const contents = fs.readFileSync(filepath, 'utf8');
    const expectedResult: TestRun = JSON.parse(contents);
    return expectedResult;
}

export function traverseDirectory(directory: AbsolutePath, onFile: (p: AbsolutePath, stats: fs.Stats) => void) {
    if (!fs.existsSync(directory)) {
        return;
    }

    const files = fs.readdirSync(directory);
    for (const file of files) {
        const fullPath = join(directory, file) as AbsolutePath;
        const stat = fs.statSync(fullPath);
        onFile(fullPath, stat);
        if (stat.isDirectory()) {
            traverseDirectory(fullPath, onFile);
        }
    }
}

//  ${storageBaseDirectory}/expected/${path-to-source-file-relative-to-workspace-root}/${functionName}/${specimenId}.json
export async function loadExpected(absoluteBaseDirectory: AbsolutePath) {
    const expecteds: Record<SpecimenId, Expected> = {};

    traverseDirectory(joinAbsolute(absoluteBaseDirectory, EXPECTED_SUBDIR), (expectedPath, _stat) => {
        const result = loadExpectedResult(expectedPath);
        expecteds[result.specimenId] = {
            result,
            expectedPath: expectedPath,
        };
    });

    return expecteds;
}

export function loadPersistedSpecimen(filepath: AbsolutePath) {
    const contents = fs.readFileSync(filepath, 'utf8');
    const specimen: Specimen = JSON.parse(contents);
    return specimen;
}

//  ${storageBaseDirectory}/specimens/${path-to-source-file-relative-to-workspace-root}/${functionName}/${specimenId}.json
//  ${storageBaseDirectory}/specimens/${path-to-source-file-relative-to-workspace-root}/${functionName}/${specimenId}.json
export async function loadPersistedSpecimens(absolutist: (r: RelativePath) => AbsolutePath, absoluteBaseDirectory: AbsolutePath) {
    const specimens: Map<SpecimenId, Specimental> = new Map();

    traverseDirectory(joinAbsolute(absoluteBaseDirectory, SPECIMENS_SUBDIR), (specimenPath, stat) => {
        if (!stat.isFile()) {
            return;
        }
        try {
            const specimen = loadPersistedSpecimen(specimenPath);
            specimens.set(specimen.id, {
                fileUnderTest: absolutist(specimen.fileUnderTest),
                specimenPath,
                specimen,
            });
        } catch (e) {
            console.error(`Error loading specimen ${specimenPath}: ${e}`);
        }
    });

    return specimens;
}

export function saveSpecimen(baseDirectory: AbsolutePath, specimental: Specimental, result?: TestRun) {
    //	TODO: don't save everything from a specimen, notably omit the leaves and any parentage
    const specimenFileAbsolutePath = join(baseDirectory, SPECIMENS_SUBDIR, `${specimental.specimen.id}.json`) as AbsolutePath;
    const specimenSubdirectory = path.dirname(specimenFileAbsolutePath);
    fs.mkdirSync(specimenSubdirectory, { recursive: true });

    fs.writeFileSync(specimenFileAbsolutePath, JSON.stringify(specimental.specimen, undefined, 2));

    if (result) {
        const resultsFileAbsolute = join(baseDirectory, EXPECTED_SUBDIR, `${specimental.specimen.id}.json`) as AbsolutePath;
        const resultsDirectory = path.dirname(resultsFileAbsolute);
        fs.mkdirSync(resultsDirectory, { recursive: true });
        fs.writeFileSync(resultsFileAbsolute, JSON.stringify(result, undefined, 2));
    }

    return specimenFileAbsolutePath;
}

export function forkSpecimen(baseDirectory: AbsolutePath, original: Specimental, newId: SpecimenId, name: string,) {
    const newSpeciment: Specimen = {
        ...original.specimen,
        type: 'custom',
        id: newId,
        name,
    };
    const newSpecimental = {
        ...original,
        specimen: newSpeciment,
    };

    const specimenFileAbsolutePath = saveSpecimen(baseDirectory, newSpecimental);
    newSpecimental.specimenPath = specimenFileAbsolutePath;
    return newSpecimental;
}


export function forkkTestCase(newTestCaseName: string, extensionState: ExtensionState, baseDirectory: AbsolutePath, specimental: Specimental) {
	const newId: SpecimenId = `custom-${newTestCaseName}`;
	const alreadyExisting = findSpecimen(extensionState, newId);
	if (alreadyExisting) {
		//	TODO: error
		return;
	}

	//	if persistable and the base test is already persisted
	if (!baseDirectory) {
		return;
	}
	// function forkTest(storageBaseDirectory: AbsolutePath, specimental: Specimental, sourceFileUnderTestPath: RelativePath, testCaseName: SpecimenId) {
	let newSpecimental: Specimental | undefined = undefined;
	if (specimental.specimenPath) {
		//	forking an already persistent test
		newSpecimental = forkSpecimen(baseDirectory, specimental, newId, newTestCaseName);
	} else {
		//	forking a transient test
		newSpecimental = {
			...specimental,
			specimen: {
				...specimental.specimen,
				id: newId,
				type: 'custom',
				name: newTestCaseName,
			},
			clusterKey: specimental.clusterKey,
			fileUnderTest: specimental.fileUnderTest,
		};
		const specimenFileAbsolutePath = saveSpecimen(baseDirectory, newSpecimental);
		newSpecimental.specimenPath = specimenFileAbsolutePath;
	}

	const nnewSpecimental = newSpecimental;
	Object.entries(extensionState.fileStates).forEach(([absoluteFileName, fileState]) => {
		if (absoluteFileName === nnewSpecimental.fileUnderTest) {
			for (const [functionName, functionState] of Object.entries(fileState.functionStates)) {
				if (functionName === specimental.specimen.functionName) {
					functionState.specimens[newId] = nnewSpecimental;
					// return;
				}
			}
		}
	});
	return newSpecimental;
}
