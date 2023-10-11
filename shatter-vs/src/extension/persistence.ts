import * as fs from 'fs'; //TODO: use VSCode fs
import * as path from 'path';
import { join } from 'path';
import { AbsolutePath, RelativePath, Specimen, SpecimenId, joinAbsolute } from '../core/common';
import { RunResult } from '../core/supervisor';
import { Expected, Specimental } from './common';

const SPECIMENS_SUBDIR = 'specimens';
const EXPECTED_SUBDIR = 'expected';

function loadExpectedResult(filepath: AbsolutePath) {
    const contents = fs.readFileSync(filepath, 'utf8');
    const expectedResult: RunResult = JSON.parse(contents);
    return expectedResult;
}

function traverseDirectory(directory: AbsolutePath, onFile: (p: AbsolutePath) => void) {
    if (!fs.existsSync(directory)) {
        return;
    }

    const files = fs.readdirSync(directory);
    for (const file of files) {
        const fullPath = join(directory, file) as AbsolutePath;
        const stats = fs.statSync(fullPath);
        if (stats.isDirectory()) {
            traverseDirectory(fullPath, onFile);
        } else {
            onFile(fullPath);
        }
    }
}

//  ${storageBaseDirectory}/expected/${path-to-source-file-relative-to-workspace-root}/${functionName}/${specimenId}.json
export async function loadExpected(absoluteBaseDirectory: AbsolutePath) {
    const expecteds: Record<SpecimenId, Expected> = {};

    traverseDirectory(joinAbsolute(absoluteBaseDirectory, EXPECTED_SUBDIR), expectedPath => {
        const result = loadExpectedResult(expectedPath);
        expecteds[result.specimenId] = {
            result,
            expectedPath: expectedPath,
        };
    })

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

    traverseDirectory(joinAbsolute(absoluteBaseDirectory, SPECIMENS_SUBDIR), specimenPath => {
        const specimen = loadPersistedSpecimen(specimenPath);
        specimens.set(specimen.id, {
            fileUnderTest: absolutist(specimen.fileUnderTest),
            specimenPath,
            specimen,
        });
    });

    return specimens;
}

export function saveTest(baseDirectory: AbsolutePath, specimental: Specimental, result?: RunResult) {
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

export function forkTest(baseDirectory: AbsolutePath, original: Specimental, newId: SpecimenId, name: string,) {
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

    const specimenFileAbsolutePath = saveTest(baseDirectory, newSpecimental);
    newSpecimental.specimenPath = specimenFileAbsolutePath;
    return newSpecimental;
}
