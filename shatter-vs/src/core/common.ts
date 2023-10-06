
//  TODO: generify value

import { createId } from "@paralleldrive/cuid2";
import { join } from "path";

export type SpecimenId = `${typeof specimenTypes[number]}-${string}`;

import { Type } from 'typescript';

//  NOT using `[${number}]` for arrays because we don't care about the position of a given value in the array
//  but we DO care about which key a given value is associated with.  TODO: re-examine the decision
//  to ignore array element position
//  include intersections because they actually create a new object; unions just pick
export type Segment = '()=>' | '.new()=>' | `["${string}"]` | '[]' | '&' | '.key' | '.value' | '.element';

//  the sequence of generators can be different in unions
export interface ObjectPathSegment {
    // type: Type,
    typeString: string,
    //  TODO: change this to an enum of union|intersection|object|literal|array|map|set|date|regexp|function|class|etc.
    generator: GeneratedParameter['generator'],
    segment: Segment,
}

export function mergePath(path: ObjectPathSegment[]): string {
    return path.map(p => p.segment).join('');
}

//  NOTE: all value objects must be serializable
interface BaseGeneratedParameter {
    id: string,
    generator: string,
    path: ObjectPathSegment[],
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
    value: string | number;   //  internal representation of enum values is always string or number
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

export interface LeafParameter {
    mergedPath: string,
    path: ObjectPathSegment[],
    value: ValueGeneratedParameter['value'],
}

export function isSpecimenId(s?: string): s is SpecimenId {
    if (s === undefined) {
        return false;
    }
    const strimmed = s.trim();
    for (const prefix of specimenTypes) {
        if (s.startsWith(prefix) && strimmed.length > prefix.length) {
            return true;
        }
    }

    return false;
}

export type Mutation = {
    path: string[],
    before: any,
    after: any,
    type: 'scramble' | 'lengthen' | 'shorten' | 'replace'
};

export const specimenTypes = ['seed', 'reduction', 'hybrid', 'mutation', 'edgication', 'custom'] as const;
export type SpecimenType = typeof specimenTypes[number];

export type BaseSpecimen = {
    parameters: GeneratedParameter[],
} & {
    type: SpecimenType
} & ({
    type: 'seed',
    // generator: string,
} | {
    type: 'reduction',
    parent: string,
} | {
    type: 'mutation',
    mutations: Mutation[],
    parent: string,
} | {
    type: 'hybrid',
    parents: string[],
} | {
    type: 'edgication',
    parents: string[],
} | {
    type: 'custom',
    name: string,
    //  TODO: in theory it could have parents, but is that useful?
});

export type AbsolutePath = `/${string}`;
//  TODO: figure out how to do RelativePath without anything BUT '/' as a prefix
export type RelativePath = `./${string}`;

export const isAbsolutePath = (path: string): path is AbsolutePath => path.startsWith('/');
export const isRelativePath = (path: string): path is RelativePath => path.startsWith('./');

export function joinAbsolute(base:AbsolutePath, relative:RelativePath):AbsolutePath {
    return join(base, relative) as AbsolutePath;
}

export type Specimen = BaseSpecimen & {
    id: SpecimenId,

	fileUnderTest: RelativePath;
	functionName: string;

    leaves: LeafParameter[],
};

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
        return gp.elements.map(extractor);
    }
    if (gp.type === 'object') {
        const o: Record<string, any> = {};
        Object.entries(gp.properties).forEach(([k, v]) => {
            o[k] = extractor(v);
        });
        return o;
    }
    if (gp.type === 'map') {
        const resolved = gp.entries.map(([k, v]): [unknown, unknown] => [extractor(k), extractor(v)]);
        if (!rehydrate) {
            return resolved;
        }
        const m = new Map(resolved);
        return m;
    }
    if (gp.type === 'set') {
        const resolved = gp.entries.map(extractor);
        if (!rehydrate) {
            return resolved;
        }
        const s = new Set(resolved);
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

export const newId = <T extends string>(type: T): `${T}-${string}` => {
    return `${type}-${createId()}`;
};

export function* findLeaves(gp: GeneratedParameter): Generator<GeneratedParameter, any, any> {
    if (gp.type === 'terminal') {
        return;
    }

    if (gp.type === 'value' || gp.type === 'date' || gp.type === 'regexp') {
        yield gp;
        return;
    }

    if (gp.type === 'array') {
        if (gp.elements) {
            for (const e of gp.elements) {
                yield* findLeaves(e);
            }
        }
        return;
    }

    if (gp.type === 'object') {
        for (const v of Object.values(gp.properties)) {
            yield* findLeaves(v);
        }
        return;
    }

    if (gp.type === 'map') {
        for (const [k, v] of gp.entries) {
            yield* findLeaves(k);
            yield* findLeaves(v);
        }
        return;
    }

    if (gp.type === 'set') {
        for (const v of gp.entries) {
            yield* findLeaves(v);
        }
        return;
    }

    if (gp.type === 'class') {
        for (const p of gp.parameters) {
            yield* findLeaves(p);
        }
        return;
    }

    if (gp.type === 'tuple') {
        for (const p of gp.values) {
            yield* findLeaves(p);
        }
        return;
    }

    if (gp.type === 'callable') {
        yield* findLeaves(gp.returnValue);
        return;
    }

    throw new Error(`Unexpected type ${gp['type']}`);
};
