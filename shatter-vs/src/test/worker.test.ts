import { work } from "../core/worker";
import { InvocationMeta } from "../core/worker-protocol";

import serializeJavascript = require("serialize-javascript");

describe('worker', () => {
    it('wwww', async () => {
        const functionName = 'foof';
        const functions = {
            [functionName]: (x:number, y:string) => 4,
        };
        
        const serializedParameters = serializeJavascript([4, functionName]);
        const message:InvocationMeta = {
            specimenId: "12412",
            launched: 0,
            invocation: {
                functionName,
                serializedParameters,
            }
        }

        console.log(`calling ${functionName} of ${Object.keys(functions)}`);
        const result = await work(functions, 3, message);
        console.log(JSON.stringify(result));
        expect(result).toBeTruthy();
        expect(result.output).toEqual(4);
    });
});