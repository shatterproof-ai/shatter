import path = require("path");
import * as fs from 'fs'; //TODO: use VSCode fs
import { AbsolutePath, RelativePath, Specimen } from "../core/common";
import { AutotestResults, shatterAutotest } from "../core/shatter";
import { ExtensionState, getActiveStates } from "./common";
import { DisplayProviders, refresh } from "./display";

function findFilesInHierarchy<K extends string>(
    absoluteFilename: AbsolutePath,
    absoluteRootDirectory: AbsolutePath,
    matchers: Record<K, (filename: string, stat: fs.Stats) => boolean>,
): Partial<Record<K, string[]>> {
    const foundFiles: Partial<Record<K, AbsolutePath[]>> = {};

    let absoluteCurrentDir = path.dirname(absoluteFilename);
    while (absoluteCurrentDir !== absoluteRootDirectory) {
        fs.readdirSync(absoluteCurrentDir).forEach((file) => {
            const absoluteFullPath = path.join(absoluteCurrentDir, file) as AbsolutePath;
            const stat = fs.statSync(absoluteFullPath);
            for (const key of Object.keys(matchers)) {
                const k: keyof typeof foundFiles = key as any;
                const matcher = matchers[k];

                const matches = matcher(absoluteFullPath, stat);
                if (matches) {
                    if (!(key in foundFiles)) {
                        foundFiles[k] = [];
                    }

                    foundFiles[k]?.push(absoluteFullPath);
                }
            }
        });

        const parentDir = path.dirname(absoluteCurrentDir);
        if (parentDir === absoluteCurrentDir) {
            break;
        }

        absoluteCurrentDir = parentDir;
    }

    return foundFiles;
}

export interface TestLifecycle {
    onTestStart: (absoluteFilename: AbsolutePath, functionName: string) => void;
    onResult: (absoluteFilename: AbsolutePath, functionName: string, result: AutotestResults) => void;
    onTestEnd: (absoluteFilename: AbsolutePath, functionName: string) => void;
}

export async function retestFunction(extensionState: ExtensionState, workspaceRoots: AbsolutePath[], absoluteSourceFilename: AbsolutePath, relativeSourceFilename: RelativePath, providers: DisplayProviders, functionName: string, specimens:Specimen[], lifeCycler: TestLifecycle, shatterproofModuleOverride: string) {
    console.log(`retestFunction ${functionName} in ${absoluteSourceFilename} with specimens ${specimens.map(s => s.id)}`);
}

export async function autotestFunction(extensionState: ExtensionState, workspaceRoots: AbsolutePath[], absoluteSourceFilename: AbsolutePath, relativeSourceFilename: RelativePath, providers: DisplayProviders, functionName: string, lifeCycler: TestLifecycle, shatterproofModuleOverride: string) {
    const _allTsConfigs: string[] = [];
    const _allPackageJsons: string[] = [];
    const allNodeModules: string[] = [];

    workspaceRoots?.forEach((absoluteFolderPath) => {
        //	TODO: do we know whether the path is already absolute always?
        //  TODO: does this even matter?
        const found = findFilesInHierarchy(absoluteSourceFilename, absoluteFolderPath, {
            tsconfig: (absoluteFilename, stat) => absoluteFilename.endsWith('tsconfig.json') && stat.isFile(),
            packageJson: (absoluteFilename, stat) => absoluteFilename.endsWith('package.json') && stat.isFile(),
            nodeModules: (absoluteFilename, stat) => absoluteFilename.endsWith('node_modules') && stat.isDirectory(),
        });

        _allTsConfigs.push(...(found.tsconfig || []));
        _allPackageJsons.push(...(found.packageJson || []));
        allNodeModules.push(...(found.nodeModules || []));
    });

    const modulePaths = [...workspaceRoots, ...allNodeModules];

    console.log(`BEGIN THE AUTOTEST of ${functionName} in ${absoluteSourceFilename}`);

    extensionState.activeCoverage = undefined;
    extensionState.activeSpecimenId = undefined;
    for (const provider of Object.values(providers)) {
        provider.refresh([]);
    }

    lifeCycler.onTestStart(absoluteSourceFilename, functionName);
    try {
        extensionState.runningAutotestFunction = functionName;

        await shatterAutotest(modulePaths,
            absoluteSourceFilename,
            relativeSourceFilename,
            functionName, (results: AutotestResults) => {
                const { fileState, functionState } = getActiveStates(extensionState);
                if (!fileState || !functionState) {
                    return;
                }

                results.clusters.forEach((cluster) => {
                    cluster.specimens.forEach((specimen) => {
                        const existing = functionState.specimens[specimen.id];
                        functionState.specimens[specimen.id] = {
                            ...existing,
                            clusterKey: cluster.key,
                            specimen,
                        };
                    });
                });

                functionState.autotest = results;
                
                lifeCycler.onResult(absoluteSourceFilename, functionName, results);

            }, { shatterproofModuleOverride, maxIterations: 50, });
        console.log("END THE AUTOTEST");
    } finally {
        lifeCycler.onTestEnd(absoluteSourceFilename, functionName);
    }
}