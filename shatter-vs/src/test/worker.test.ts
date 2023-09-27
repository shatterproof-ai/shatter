import { GeneratedParameter, extractGeneratedParameterValue } from "../core/common";
import { work } from "../core/worker";
import { InvocationMeta } from "../core/worker-protocol";

import serializeJavascript = require("serialize-javascript");

describe('worker', () => {
    it('wwww', async () => {
        const functionName = 'foof';
        const functions = {
            [functionName]: (x: number, y: string) => 4,
        };

        const parameters: GeneratedParameter[] = [{
            id: '1',
            type: 'value',
            generator: 'tizzest',
            value: 4
        }];

        const resolvedParameters = parameters.map(extractGeneratedParameterValue);

        const serializedParameters = serializeJavascript(resolvedParameters);
        const message: InvocationMeta = {
            specimenId: "12412",
            launched: 0,
            invocation: {
                functionName,
                serializedParameters,
                parameters,
            }
        }

        console.log(`calling ${functionName} of ${Object.keys(functions)}`);
        const result = await work(functions, 3, message);
        console.log(JSON.stringify(result));
        expect(result).toBeTruthy();
        expect(result.output).toEqual(4);
    });
});