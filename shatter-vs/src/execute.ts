import { ChildProcessWithoutNullStreams, spawn } from "child_process";
import { parse, join } from "path";
//  https://github.com/next-gen-dev/vscode-run-function/blob/main/src/runner.ts
function tsProcess(filepath: string, functionName: string) {
    const { name, dir } = parse(filepath);
    const bin = join(
        __dirname,
        "..",
        "node_modules",
        "ts-node",
        "dist",
        "bin.js",
    );
    const args = [
        "-T",
        "-O",
        `{"target": "es2015", "module": "commonjs"}`,
        "-e",
        `import('./${name}').then(m => m.${functionName}()).then(v => console.log(JSON.stringify(v, null, 4)), console.error)`,
    ];
    console.log(`Executing: ${bin} ${args.join(" ")}`);
    return spawn(bin, args, { cwd: dir });
}