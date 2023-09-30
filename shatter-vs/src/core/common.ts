
//  TODO: generify value

import { createId } from "@paralleldrive/cuid2";

//  NOTE: all value objects must be serializable
interface BaseGeneratedParameter {
    id: string,
    generator: string,
    options?: Record<string, any>,
}

//  TODO: isn't this the same as simple types?
export const valueSubtypes = ["string", "number", "enum", "boolean", "undefined", "null"] as const;
export type ValueSubtype = typeof valueSubtypes[number];

export const isValueSubtype = (s: string): s is ValueSubtype => {
    return valueSubtypes.includes(s as ValueSubtype);
};

interface BaseValueGeneratedParameter extends BaseGeneratedParameter {
    subtype: ValueSubtype,
    type: 'value',
}

export interface StringGeneratedParameter extends BaseValueGeneratedParameter {
    subtype: 'string',
    value: string,
}

export interface NumberGeneratedParameter extends BaseValueGeneratedParameter {
    subtype: 'number',
    value: number,
}

export interface BooleanGeneratedParameter extends BaseValueGeneratedParameter {
    subtype: 'boolean',
    value: boolean,
}

export interface UndefinedGeneratedParameter extends BaseValueGeneratedParameter {
    subtype: 'undefined',
    value: undefined,
}

export interface NullGeneratedParameter extends BaseValueGeneratedParameter {
    subtype: 'null',
    value: null,
}

export interface EnumGeneratedParameter extends BaseValueGeneratedParameter {
    subtype: 'enum',
    value: any;
}

export type ValueGeneratedParameter = (StringGeneratedParameter | NumberGeneratedParameter | BooleanGeneratedParameter | UndefinedGeneratedParameter | NullGeneratedParameter | EnumGeneratedParameter);

export interface ArrayGeneratedParameter extends BaseGeneratedParameter {
    type: 'array',
    elements: GeneratedParameter[],
}

export interface TupleGeneratedParameter extends BaseGeneratedParameter {
    type: 'tuple',
    values: GeneratedParameter[],
}

export interface IntersectionGeneratedParameter extends BaseGeneratedParameter {
    type: 'intersection',
    parts: GeneratedParameter[],
}

export interface ClassGeneratedParameter extends BaseGeneratedParameter {
    type: 'class',
    instance: any,
}

export interface MapGeneratedParameter extends BaseGeneratedParameter {
    type: 'map',
    entries: [GeneratedParameter, GeneratedParameter][],
}

export interface SetGeneratedParameter extends BaseGeneratedParameter {
    type: 'set',
    entries: GeneratedParameter[],
}

export interface DateGeneratedParameter extends BaseGeneratedParameter {
    type: 'date',
    epochMs: number,
}

export interface RegExpGeneratedParameter extends BaseGeneratedParameter {
    type: 'regexp',
    pattern: string,
}

export interface ConstructorGeneratedParameter extends BaseGeneratedParameter {
    type: 'constructor',
    constructed: GeneratedParameter,
}

export interface CallableGeneratedParameter extends BaseGeneratedParameter {
    type: 'callable',
    returnValue: GeneratedParameter,
}

export interface TerminalGeneratedParameter extends BaseGeneratedParameter {
    //  For when the object graph goes on but we can't
    type: 'terminal',
}

export interface ObjectGeneratedParameter extends BaseGeneratedParameter {
    type: 'object',
    properties: Record<string, GeneratedParameter>,
    required: string[],
    declaredType: string,
}

export type GeneratedParameter = ValueGeneratedParameter | ArrayGeneratedParameter | TupleGeneratedParameter | IntersectionGeneratedParameter | ClassGeneratedParameter | MapGeneratedParameter | SetGeneratedParameter | DateGeneratedParameter | RegExpGeneratedParameter | ConstructorGeneratedParameter | CallableGeneratedParameter | TerminalGeneratedParameter | ObjectGeneratedParameter;

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