
//  TODO: generify value

import { createId } from "@paralleldrive/cuid2";

//  NOTE: all value objects must be serializable
export type GeneratedParameter = {
    id: string,
    generator: string,
    options?: Record<string, any>,
} & ({
    type: 'value',
    value: any,
} | {
    type: 'array',
    elements: GeneratedParameter[],
} | {
    type: 'tuple',
    values: GeneratedParameter[],
} | {
    type: 'intersection',
    parts: GeneratedParameter[],
} | {
    type: 'class',
    instance: any,
} | {
    type: 'map',
    entries: [GeneratedParameter, GeneratedParameter][],
} | {
    type: 'set',
    entries: GeneratedParameter[],
} | {
    type: 'date',
    epochMs: number,
} | {
    type: 'regexp',
    pattern: string,
} | {
    type: 'constructor',
    constructed: GeneratedParameter,
} | {
    type: 'callable',
    returnValue: GeneratedParameter,
} | {
    //  For when the object graph goes on but we can't
    type: 'terminal',
} | {
    type: 'object',
    properties: Record<string, GeneratedParameter>,
    required: string[],
    declaredType: string,
});

export const extractGeneratedParameterValue = (gp: GeneratedParameter): any => {
    if (gp.type === 'terminal') {
        return undefined;
    }
    if (gp.type === 'value') {
        return gp.value;
    }
    if (gp.type === 'array') {
        if (gp.elements) {
            return gp.elements.map(extractGeneratedParameterValue);
        }
        throw new Error(`Unexpected missing elements in array gp ${JSON.stringify(gp)}`);
    }
    if (gp.type === 'object') {
        const o: Record<string, any> = {};
        Object.entries(gp.properties).forEach(([k, v]) => {
            o[k] = extractGeneratedParameterValue(v);
        });
        return o;
    }
    if (gp.type === 'map') {
        const m = new Map();
        gp.entries.forEach(([k, v]) => {
            const key = extractGeneratedParameterValue(k);
            const value = extractGeneratedParameterValue(v);
            m.set(key, value);
        });
        return m;
    }
    if (gp.type === 'set') {
        const s = new Set();
        gp.entries.forEach((v) => {
            const value = extractGeneratedParameterValue(v);
            s.add(value);
        });
        return s;
    }
    if (gp.type === 'date') {
        return new Date(gp.epochMs);
    }
    if (gp.type === 'regexp') {
        return new RegExp(gp.pattern);
    }
    if (gp.type === 'class') {
        return gp.instance;
    }
    if (gp.type === 'tuple') {
        return gp.values.map(extractGeneratedParameterValue);
    }
    if (gp.type === 'constructor') {
        const v = extractGeneratedParameterValue(gp.constructed);
        return (_:any) => v;
    }
    if (gp.type === 'callable') {
        const v = extractGeneratedParameterValue(gp.returnValue);
        return (_:any) => {
            return v;
        };
    }
    if (gp.type === 'intersection') {
        const merged: any = {};
        for (const part of gp.parts) {
            const o = extractGeneratedParameterValue(part);
            Object.assign(merged, o);
        }
        return merged;
    }
    throw new Error(`Unexpected type ${gp['type']}`);
};

export const compressRanges = (lines: number[]) => {
	let currentRangeStart = lines[0];
	let currentRangeEnd = lines[0];
	const compressedRanges: [number, number][] = [];
	for (let i = 1; i < lines.length; i++) {
		if (currentRangeStart + 1 === lines[i]) {
			currentRangeEnd = lines[i];
		} else {
			compressedRanges.push([currentRangeStart, currentRangeEnd]);
			currentRangeStart = lines[i];
			currentRangeEnd = lines[i];
		}
	}

	return compressedRanges;
};

export const skip = <T, U, V>(g:Iterator<T, U, V>, n:number):T|undefined => {
    let latest:T|undefined = undefined;
    let i = 0;
    for (; i < n + 1; i++) {
        const it = g.next();
        if (it.done) {
            break;
        }
        latest = it.value;
    }
    return latest;
};

export const newId = (type:string):string => {
    return `${type}-${createId()}`;
};