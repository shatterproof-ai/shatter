
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

export interface ClassGeneratedParameter extends BaseGeneratedParameter {
    type: 'class',
    fullyQualifiedName: string,
    parameters: GeneratedParameter[],
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

export type GeneratedParameter = ValueGeneratedParameter | ArrayGeneratedParameter | TupleGeneratedParameter | ClassGeneratedParameter | MapGeneratedParameter | SetGeneratedParameter | DateGeneratedParameter | RegExpGeneratedParameter | CallableGeneratedParameter | TerminalGeneratedParameter | ObjectGeneratedParameter;

const resolveGeneratedParameterValue = (gp: GeneratedParameter, rehydrate: boolean, activeModule: any): any => {
    function extractor(gp: GeneratedParameter): any {
        return resolveGeneratedParameterValue(gp, rehydrate, activeModule);
    }

    if (gp.type === 'terminal') {
        return undefined;
    }
    if (gp.type === 'value') {
        return gp.value;
    }
    if (gp.type === 'array') {
        if (gp.elements) {
            return gp.elements.map(extractor);
        }
        throw new Error(`Unexpected missing elements in array gp ${JSON.stringify(gp)}`);
    }
    if (gp.type === 'object') {
        const o: Record<string, any> = {};
        Object.entries(gp.properties).forEach(([k, v]) => {
            o[k] = extractor(v);
        });
        return o;
    }
    if (gp.type === 'map') {
        if (!rehydrate) {
            return gp.entries;
        }
        const m = new Map();
        gp.entries.forEach(([k, v]) => {
            const key = extractor(k);
            const value = extractor(v);
            m.set(key, value);
        });
        return m;
    }
    if (gp.type === 'set') {
        if (!rehydrate) {
            return gp.entries;
        }
        const s = new Set();
        gp.entries.forEach((v) => {
            const value = extractor(v);
            s.add(value);
        });
        return s;
    }
    if (gp.type === 'date') {
        if (rehydrate) {
            return new Date(gp.epochMs);
        }
        return gp.epochMs;
    }
    if (gp.type === 'regexp') {
        if (!rehydrate) {
            return gp.pattern;
        }
        return new RegExp(gp.pattern);
    }
    if (gp.type === 'class') {
        //  We are guaranteed that the given class is in the current/global
        //  scope because it's declared in the signature of the function under test
        if (!rehydrate) {
            return {
                className: gp.fullyQualifiedName,
                parameters: gp.parameters.map(extractor),
            };
        }

        const classRef = (activeModule as any)[gp.fullyQualifiedName];
        if (!classRef) {
            const keyses = Object.keys(activeModule);
            const exportses = Object.keys(activeModule);
            throw new Error(`Class ${gp.fullyQualifiedName} not found in module scope; available keys: ${keyses.join(', ')}, exportses = ${exportses.join(', ')}}`);
        }

        const resolvedParameters = gp.parameters.map(extractor);

        const instance = new classRef(resolvedParameters);
        return instance;
    }
    if (gp.type === 'tuple') {
        return gp.values.map(extractor);
    }
    if (gp.type === 'callable') {
        if (!rehydrate) {
            return {
                returnValue: extractor(gp.returnValue),
            };
        }
        const v = extractor(gp.returnValue);
        return (_: any) => {
            return v;
        };
    }

    throw new Error(`Unexpected type ${gp['type']}`);
};

export const extractGeneratedParameterValue = (gp: GeneratedParameter): any =>
    resolveGeneratedParameterValue(gp, false, {});

export const rehydrateGeneratedParameterValue = (gp: GeneratedParameter, activeModule: any): any =>
    resolveGeneratedParameterValue(gp, true, activeModule);

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

export const skip = <T, U, V>(g: Iterator<T, U, V>, n: number): T | undefined => {
    let latest: T | undefined = undefined;
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

export const newId = (type: string): string => {
    return `${type}-${createId()}`;
};

//  TODO: fix this name; it's a path + value
interface FieldExtract {
    path: string[],
    value: any,
}

export const vectorizeParameter = (gp: GeneratedParameter, path: string[]): FieldExtract[] => {
    if (gp.type === 'terminal') {
        return [];
    }

    if (gp.type === 'value') {
        return [{
            path,
            value: gp.value,
        }]
    }

    if (gp.type === 'date') {
        return [{
            path,
            value: gp.epochMs,
        }]
    }

    if (gp.type === 'regexp') {
        return [{
            path,
            value: gp.pattern,
        }]
    }

    if (gp.type === 'array') {
        if (gp.elements) {
            const vv = gp.elements.flatMap((e, i) => vectorizeParameter(e, [...path, `${i}`]));
            return vv;
        }
        return [];
    }

    if (gp.type === 'object') {
        const vv = Object.entries(gp.properties).flatMap(([k, v]) => vectorizeParameter(v, [...path, `["${k}"]`]));
        return vv;
    }

    if (gp.type === 'map') {
        const vv = gp.entries.flatMap(([k, v]) => {
            const key = vectorizeParameter(k, [...path, '.key']);
            const value = vectorizeParameter(v, [...path, '.value']);
            return [...key, ...value];
        })
        return vv;
    }

    if (gp.type === 'set') {
        const vv = gp.entries.flatMap(e => vectorizeParameter(e, [...path, '.element']));
        return vv;
    }

    if (gp.type === 'class') {
        const vv = gp.parameters.flatMap((p, i) => vectorizeParameter(p, [...path, `.${i}`]));
        return vv;
    }

    if (gp.type === 'tuple') {
        const vv = gp.values.flatMap((p, i) => vectorizeParameter(p, [...path, `.${i}`]));
        return vv;
    }

    if (gp.type === 'callable') {
        return [{
            path: [...path, '.()'],
            value: gp.returnValue,
        }];
    }

    throw new Error(`Unexpected type ${gp['type']}`);
};
