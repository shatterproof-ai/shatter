import path = require("path");
import * as fs from 'fs'; //TODO: use VSCode fs
import { AbsolutePath, RelativePath, Specimen } from "../core/common";
import { AutotestResults, RunUpdate, shatterAutotest, shatterRetest } from "../core/shatter";
import { ExtensionState } from "./common";
import { DisplayProviders } from "./display";
import { traverseDirectory } from "./persistence";

function findFilesInHierarchy<K extends string>(
    absoluteRootDirectory: AbsolutePath,
    matchers: Record<K, (filename: string, stat: fs.Stats) => boolean>,
): Partial<Record<K, string[]>> {
    const foundFiles: Partial<Record<K, AbsolutePath[]>> = {};

    traverseDirectory(absoluteRootDirectory, (absoluteFullPath, stat) => {
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

    return foundFiles;
}

export interface TestLifecycle {
    onTestStart: (absoluteFilename: AbsolutePath, functionName: string) => void;
    onResult: (absoluteFilename: AbsolutePath, functionName: string, result: AutotestResults) => void;
    onTestEnd: (absoluteFilename: AbsolutePath, functionName: string) => void;
}

export async function retestFunction(extensionState: ExtensionState, workspaceRoots: AbsolutePath[], absoluteSourceFilename: AbsolutePath, functionName: string, specimens: Specimen[], lifeCycler: TestLifecycle, shatterproofModuleOverride: string) {
    const allNodeModules: string[] = findKeyFiles(workspaceRoots, absoluteSourceFilename);

    const modulePaths = [...workspaceRoots, ...allNodeModules];

    lifeCycler.onTestStart(absoluteSourceFilename, functionName);
    try {
        extensionState.runningTestFunction = functionName;

        await shatterRetest(modulePaths,
            absoluteSourceFilename,
            functionName, specimens,
            (update: RunUpdate, results: AutotestResults) => {
                const fileState = extensionState.fileStates[absoluteSourceFilename];
                if (!fileState) {
                    return;
                }

                const functionState = fileState.functionStates[functionName];
                if (!functionState) {
                    return;
                }

                //  copy everything over to functionState
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

                //  do not overwrite functionState.autotest on retest
                functionState.autotest.instrumentedLines = results.instrumentedLines;
                const existingCluster = functionState.autotest.clusters.find((c) => c.key === update.cluster.key);
                if (!existingCluster) {
                    //  haven't previously seen the relevant cluster
                    functionState.autotest.clusters.push(update.cluster);
                }

                lifeCycler.onResult(absoluteSourceFilename, functionName, results);

            }, { shatterproofModuleOverride });
    } finally {
        lifeCycler.onTestEnd(absoluteSourceFilename, functionName);
    }
}

function findKeyFiles(workspaceRoots: `/${string}`[], absoluteSourceFilename: string) {
    const _allTsConfigs: string[] = [];
    const _allPackageJsons: string[] = [];
    const allNodeModules: string[] = [];

    //  TODO: this is a crude guess at the resolution path that will almost certainly break
    //  in more complex cases
    workspaceRoots?.forEach((absoluteFolderPath) => {
        //	TODO: do we know whether the path is already absolute always?
        //  TODO: does this even matter?
        const found = findFilesInHierarchy(absoluteFolderPath, {
            tsconfig: (absoluteFilename, stat) => path.basename(absoluteFilename) === 'tsconfig.json' && stat.isFile(),
            packageJson: (absoluteFilename, stat) => path.basename(absoluteFilename) === 'package.json' && stat.isFile(),
            nodeModules: (absoluteFilename, stat) => path.basename(absoluteFilename) === 'node_modules' && stat.isDirectory(),
        });

        _allTsConfigs.push(...(found.tsconfig || []));
        _allPackageJsons.push(...(found.packageJson || []));
        allNodeModules.push(...(found.nodeModules || []));
    });
    return allNodeModules;
}

export async function autotestFunction(extensionState: ExtensionState, workspaceRoots: AbsolutePath[], absoluteSourceFilename: AbsolutePath, relativeSourceFilename: RelativePath, providers: DisplayProviders, functionName: string, lifeCycler: TestLifecycle, shatterproofModuleOverride: string) {
    const allNodeModules: string[] = findKeyFiles(workspaceRoots, absoluteSourceFilename);

    process.chdir(path.dirname(absoluteSourceFilename));
    const modulePaths = [...workspaceRoots, ...allNodeModules];

    console.log(`BEGIN THE AUTOTEST of ${functionName} in ${absoluteSourceFilename}`);

    for (const provider of Object.values(providers)) {
        provider.refresh([]);
    }

    lifeCycler.onTestStart(absoluteSourceFilename, functionName);
    try {
        extensionState.runningTestFunction = functionName;

        await shatterAutotest(modulePaths,
            absoluteSourceFilename,
            relativeSourceFilename,
            functionName, (update: RunUpdate, results: AutotestResults) => {
                const fileState = extensionState.fileStates[absoluteSourceFilename];
                if (!fileState) {
                    return;
                }

                const functionState = fileState.functionStates[functionName];
                if (!functionState) {
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