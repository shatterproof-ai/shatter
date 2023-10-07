import * as fs from 'fs'; //TODO: use VSCode fs
import * as path from 'path';
import { join } from 'path';
import { AbsolutePath, RelativePath, Specimen, SpecimenId, isSpecimenId } from '../core/common';
import { RunResult } from '../core/supervisor';
import { Specimental } from './common';

export function loadPersistedSpecimen(filepath: AbsolutePath) {
    const contents = fs.readFileSync(filepath, 'utf8');
    const specimen: Specimen = JSON.parse(contents);
    return specimen;
}

export function loadPersistedSpecimens(absolutist: (r:RelativePath) => AbsolutePath, specimenDirectory: AbsolutePath) {
    const suffix = '.json';
    const targetSubdirNames = ['custom', 'autotest'];
    const specimens: Map<SpecimenId, Specimental> = new Map();

    function traverseDirectory(directory: AbsolutePath, targetSubdirName: string) {
        if (!fs.existsSync(directory)) {
            return;
        }

        const files = fs.readdirSync(directory);
        for (const file of files) {
            const fullPath = join(directory, file) as AbsolutePath;
            const stats = fs.statSync(fullPath);
            if (stats.isDirectory()) {
                if (file === targetSubdirName) {
                    const targetSubdirContents = fs.readdirSync(fullPath);
                    for (const leafFile of targetSubdirContents) {
                        if (leafFile.endsWith(suffix)) {
                            const specimenId = leafFile.slice(0, -suffix.length);
                            if (isSpecimenId(specimenId)) {
                                const specimenPath = join(fullPath, leafFile) as AbsolutePath;
                                const specimen = loadPersistedSpecimen(specimenPath);
                                specimens.set(specimenId, {
                                    fileUnderTest: absolutist(specimen.fileUnderTest),
                                    specimenPath,
                                    specimen,
                                });
                                //	TODO: load corresponding results
                            }
                        }
                    }
                } else {
                    traverseDirectory(fullPath, targetSubdirName);
                }
            }
        }
    }

    for (const targetSubdirName of targetSubdirNames) {
        traverseDirectory(specimenDirectory, targetSubdirName);
    }

    return specimens;
}

export function saveTest(specimenBaseDirectory: AbsolutePath, specimental: Specimental, result?: RunResult) {
    //	TODO: don't save everything from a specimen, notably omit the leaves and any parentage
    const specimenSubdir = specimental.specimen.id.startsWith('custom') ? 'custom' : 'autotest';
    const specimenFileAbsolutePath = join(specimenBaseDirectory, 'specimens', specimenSubdir, `${specimental.specimen.id}.json`) as AbsolutePath;
    const specimenSubdirectory = path.dirname(specimenFileAbsolutePath);
    fs.mkdirSync(specimenSubdirectory, { recursive: true });
    fs.writeFileSync(specimenFileAbsolutePath, JSON.stringify(specimental.specimen, undefined, 2));

    if (result) {
        const resultsFileAbsolute = join(specimenBaseDirectory, 'results', specimenSubdir, `${specimental.specimen.id}.json`) as AbsolutePath;
        const resultsDirectory = path.dirname(resultsFileAbsolute);
        fs.mkdirSync(resultsDirectory, { recursive: true });
        fs.writeFileSync(resultsFileAbsolute, JSON.stringify(result, undefined, 2));
    }

    return specimenFileAbsolutePath;
}

export function forkTest(specimenBaseDirectory: AbsolutePath, original: Specimental, newId: SpecimenId, name: string,) {
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

    const specimenFileAbsolutePath = saveTest(specimenBaseDirectory, newSpecimental);
    newSpecimental.specimenPath = specimenFileAbsolutePath;
    return newSpecimental;
}
