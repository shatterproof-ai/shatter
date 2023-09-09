import * as ts from "typescript";
import { shatterAutotest } from "../shatter";
import { fail } from "assert";

describe('extension', () => {
    it('should pass', async () => {
        const testfile = "/home/ketan/tmp/hello.ts";
        const program = ts.createProgram([testfile], {});
        const source = program.getSourceFile(testfile);
        if (!source) {
            fail(`Could not find source file ${testfile}`);
        }

        const functionName = "hello";

        let functionNode: ts.FunctionDeclaration | null = null;
        const visitor = (node: ts.Node) => {
            if (ts.isFunctionDeclaration(node)) {
                functionNode = node;
                return node;
            }
            ts.forEachChild(node, visitor);
        };

        ts.forEachChild(source, visitor);

        if (!functionNode) {
            fail(`Could not find function ${functionName}`);
        }

        const modulePaths = process.env.NODE_ENV?.split(':') ?? [];
        shatterAutotest(modulePaths, functionNode, (clusters) => {
            console.log(`Received clusters ${JSON.stringify(clusters)}`);
        });
    });
});