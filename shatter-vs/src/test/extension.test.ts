
import { mkdtempSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

import { shatterAutotest } from "../shatter";

describe('extension', () => {
    it('should pass', async () => {
        const sourceCode = `
        function hello(n:number, msg:string) {
            if (n == 0) {
                throw new Error("n must be at least 1");
            }
            
            const pieces:string[] = []
            
            for (let i = 0; i < n; i++) {
                pieces.push(msg)
            }

            if (i % 2 == 0) {
                return ", ".join(pieces)
            }
            return pieces.join("; ")
        }
        `;

        const tempdir = mkdtempSync(join(tmpdir(), "shatter-test-"));
        const testfile = join(tempdir, 'hello.ts');
        writeFileSync(testfile, sourceCode);

        const functionName = "hello";

        const modulePaths = process.env.NODE_ENV?.split(':') ?? [];

        await shatterAutotest(modulePaths, testfile, functionName, (clusters) => {
            console.log(`Received clusters ${JSON.stringify(clusters)}`);
        }, "/home/ketan/project/shatter/shatter-vs/src");
    });
});