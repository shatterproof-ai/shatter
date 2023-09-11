
import { mkdtempSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

import { shatterAutotest } from "../shatter";

describe('extension', () => {
    it('should pass', async () => {
        const sourceCode = `
        function hello(n:number, msg:string) {
            if (n <= 0) {
                throw new Error("n must be at least 1");
            }
        
            if (n % 1 != 0) {
                throw new Error("n must be an integer");
            }
        
            const pieces:string[] = []
            
            switch (n) {
                case 3: {
                    console.log("n is 3");
                    break;
                }
                case 6: {
                    console.log("n is 6");
                    break;
                }
                case 10: {
                    console.log("n is 10");
                    break;
                }
                case 15: {
                    console.log("n is 15");
                    break;
                }
                case 40:
                case 41: {
                    console.log("n is 40 or 41");
                    break;
                }
            }

            for (let i = 0; i < n; i++) {
                if (i > 50) {
                    break;
                }
                pieces.push(msg)
            }
        
            if (n % 2 == 0) {
                return pieces.join(", ")
            }
            return pieces.join("; ")
        }
        `;

        const tempdir = mkdtempSync(join(tmpdir(), "shatter-test-"));
        const testfile = join(tempdir, 'hello.ts');
        writeFileSync(testfile, sourceCode);

        const functionName = "hello";

        const modulePaths = process.env.NODE_ENV?.split(':') ?? [];

        await shatterAutotest(modulePaths, testfile, tempdir, functionName, (clusters) => {
            console.log(`Received clusters ${JSON.stringify(clusters)}`);
        }, "/home/ketan/project/shatter/shatter-vs/src");
    });
});