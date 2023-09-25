import * as ts from 'typescript';

import { createId } from "@paralleldrive/cuid2";

import { distance as levenshtein } from 'fastest-levenshtein';
import { ResultCluster } from '../core/shatter';
import { RunResult } from '../core/supervisor';
import path = require('path');
import { GeneratedParameter, edgyAny, edgyStrings, edgyNumbers, edgyBooleans } from './seed';

export type Mutation = {
    path: string[],
    before: any,
    after: any,
    type: 'scramble' | 'lengthen' | 'shorten' | 'replace'
};

export type BaseSpecimen = {
    parameters: GeneratedParameter[],
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
});

export type Specimen = BaseSpecimen & {
    id: string,
    sequence: number,
};

export interface GeneratedParameterList {
    id: string,
    sequence: number
    parameters: any[]
}

interface TestCaseSource {
    start(): Iterator<GeneratedParameterList>;
    increaseWeirdness?(): void;
    update?(clusterMap: Map<string, ResultCluster>, r: RunResult): void;
}

export class RetestCaseSource implements TestCaseSource {
    private clusterIndex = 0;
    private resultIndex = 0;
    private counter = 0;
    constructor(private clusters: ResultCluster[]) { }
    *start(): Iterator<GeneratedParameterList> {

        if (this.clusterIndex < this.clusters.length
            && this.resultIndex >= this.clusters[this.clusterIndex].results.length) {
            this.clusterIndex++;
            this.resultIndex = 0;
        }

        if (this.clusterIndex >= this.clusters.length) {
            return;
        }

        const result = this.clusters[this.clusterIndex].results[this.resultIndex];
        this.resultIndex++;
        //  TODO: should this save GeneratedParameterList instead of the bare parameters any[]?
        yield {
            id: createId(),
            sequence: this.counter++,
            parameters: result.parameters,
        };
    }
}

export const comparameters = (a: any, b: any): number => {
    //  null and undefined always sort to the end
    if (a === null || a === undefined) {
        return 1;
    }
    if (b === null || b === undefined) {
        return -1;
    }

    if (typeof a !== typeof b) {
        //  TODO
        return 0;
    }

    if (typeof a === 'string') {
        return a.localeCompare(b);
    }

    if (typeof a === 'number') {
        return a - b;
    }

    if (typeof a === 'boolean') {
        if (a === b) {
            return 0;
        }
        return a ? 1 : -1;
    }

    if (typeof a === 'object') {
        if (Array.isArray(a)) {
            for (let i = 0; i < a.length && i < b.length; i++) {
                const cmp = comparameters(a[i], b[i]);
                if (cmp !== 0) {
                    return cmp;
                }
            }
            return a.length - b.length;
        }

        const akeys = Object.keys(a).sort();
        const bkeys = Object.keys(b).sort();

        //  looking at common keys first is an arbitrary decision that can/should be questioned
        //  which method is best at finding differences?
        const commonKeys = akeys.filter(k => bkeys.includes(k));
        for (const key of commonKeys) {
            const cmp = comparameters(a[key], b[key]);
            if (cmp !== 0) {
                return cmp;
            }
        }
        for (const key of akeys) {
            if (!commonKeys.includes(key)) {
                return -1;
            }
        }
        for (const key of bkeys) {
            if (!commonKeys.includes(key)) {
                return 1;
            }
        }
        return 0;
    }

    throw new Error(`Unexpected type ${typeof a}`);
};

export function computeDistance(a: any, b: any): number {
    if (a === b || a === null || b === null || a === undefined || b === undefined) {
        return 0;
    }

    if (typeof a === 'number') {
        const smaller = Math.min(a, b);
        const larger = Math.max(a, b);
        const difference = larger - smaller;
        if (difference === 0) {
            return 0;
        }
        if (difference < 2 && Number.isInteger(a) && Number.isInteger(b)) {
            return 1;
        }
        return difference;
    }

    if (typeof a === 'string') {
        const dist = levenshtein(a, b);
        return dist;
    }

    if (typeof a === 'boolean') {
        return a === b ? 0 : 1;
    }

    //  TODO: the array and object versions may go too far down irrelevant rabbit holes
    if (Array.isArray(a)) {
        const arrayDistance = a.reduce((acc, val, index) => acc + computeDistance(val, b[index]), 0);
        return arrayDistance;
    }

    if (typeof a === 'object') {
        const akeys = Object.keys(a);
        const bkeys = Object.keys(b);
        const commonKeys = akeys.filter(k => bkeys.includes(k));
        const objectDistance = commonKeys.reduce((acc, key) => acc + computeDistance(a[key], b[key]), 0);
        return objectDistance;
    }

    throw new Error(`Unexpected type ${typeof a}`);
}

const isObjectType = (type: ts.Type): type is ts.ObjectType => {
    return (type as ts.ObjectType).objectFlags !== undefined;
};

const isTypeReference = (type: ts.Type): type is ts.TypeReference => {
    return isObjectType(type)
        && type.objectFlags === ts.ObjectFlags.Reference;
};

const isEnumType = (type: ts.Type): type is ts.EnumType => {
    //  TODO: when will this be Enum and when EnumLiteral?
    return type.flags === ts.TypeFlags.Enum || type.flags === ts.TypeFlags.EnumLiteral;
};


interface ValueGenerator {
    generate: () => Generator<GeneratedParameter, any, any>;
}

type Sizer = (o?: any) => Generator<number, any, any>;
type PropertyPicker = (k: string[]) => Generator<string[], any, any>;
type ElementPicker = (max: number) => Generator<number, any, any>;

class LiteralValueGenerator implements ValueGenerator {
    constructor(private value: any) { }
    *generate(): Generator<GeneratedParameter, any, any> {
        while (true) {
            yield {
                id: createId(),
                generator: this.constructor.name,
                type: 'value',
                value: this.value,
            };
        }
    }
}

class SimpleValueGenerator implements ValueGenerator {
    constructor(private flags: ts.TypeFlags) { }

    *generate(): Generator<GeneratedParameter, any, any> {
        while (true) {
            switch (this.flags) {
                case ts.TypeFlags.Any:
                case ts.TypeFlags.Unknown:
                    yield* edgyAny();
                    break;
                case ts.TypeFlags.String:
                    yield* edgyStrings();
                    break;
                case ts.TypeFlags.Number:
                    yield* edgyNumbers();
                    break;
                case ts.TypeFlags.Boolean:
                    yield* edgyBooleans();
                    break;
                default:
                    throw new Error(`Unexpected type ${this.flags}`);
            }
        }
    }
}

class RoundRobinValueGenerator implements ValueGenerator {
    constructor(private values: any[]) { }

    *generate(): Generator<GeneratedParameter, any, any> {
        while (true) {
            for (const v of this.values) {
                yield {
                    id: createId(),
                    generator: this.constructor.name,
                    type: 'value',
                    value: v,
                };
            }
        }
    }
}

//  composite generators
class SimpleObjectGenerator implements ValueGenerator {
    constructor(
        private propertyGenerators: Record<string, Generator<GeneratedParameter, any, any>>,
        private picker?: PropertyPicker,
    ) { }

    *generate(): Generator<GeneratedParameter, any, any> {
        const allProperties = Object.keys(this.propertyGenerators);
        const keysGenerator = this.picker
            ? this.picker(allProperties)
            : function* () {
                while (true) {
                    yield allProperties;
                }
            }();

        while (true) {
            for (const keys of keysGenerator) {
                const o: Record<string, GeneratedParameter> = {};
                for (const k of keys) {
                    const key = k as string;
                    const next = this.propertyGenerators[key].next();
                    if (next.done) {
                        throw new Error(`Generator ${key} is done`);
                    }

                    o[key] = next.value;
                }

                yield {
                    id: createId(),
                    generator: this.constructor.name,
                    type: 'object',
                    properties: o,
                };
            }
        }
    }
}

class ArrayGenerator implements ValueGenerator {
    constructor(
        private elementGenerator: Generator<GeneratedParameter, any, any>,
        private sizer: Sizer,
    ) { }

    *generate(): Generator<GeneratedParameter, any, any> {
        while (true) {
            for (const count of this.sizer()) {
                const a = [];
                for (let i = 0; i < count; i++) {
                    const next = this.elementGenerator.next();
                    if (next.done) {
                        throw new Error(`Generator ${this.elementGenerator.constructor.name} is done`);
                    }
                    a.push(next.value);
                }

                yield {
                    id: createId(),
                    generator: this.constructor.name,
                    type: 'array',
                    range: a,
                };
            }
        }
    }
}

class MapGenerator implements ValueGenerator {
    constructor(
        private keyGenerator: Generator<GeneratedParameter, any, any>,
        private valueGenerator: Generator<GeneratedParameter, any, any>,
        private sizer: Sizer,
    ) { }

    *generate(): Generator<GeneratedParameter, any, any> {
        while (true) {
            for (const count of this.sizer()) {
                const m = new Map();
                for (let i = 0; i < count; i++) {
                    const key = this.keyGenerator.next();
                    if (key.done) {
                        throw new Error(`Generator ${this.keyGenerator.constructor.name} is done`);
                    }
                    const value = this.valueGenerator.next();
                    if (value.done) {
                        throw new Error(`Generator ${this.valueGenerator.constructor.name} is done`);
                    }
                    m.set(key.value, value.value);
                }
                yield {
                    id: createId(),
                    generator: this.constructor.name,
                    type: 'class',
                    instance: m,
                };
            }
        }
    }
}

class SetGenerator implements ValueGenerator {
    constructor(
        private elementGenerator: Generator<GeneratedParameter, any, any>,
        private sizer: Sizer,
    ) { }

    *generate(): Generator<GeneratedParameter, any, any> {
        while (true) {
            for (const count of this.sizer()) {
                const s = new Set();
                for (let i = 0; i < count; i++) {
                    const next = this.elementGenerator.next();
                    if (next.done) {
                        throw new Error(`Generator ${this.elementGenerator.constructor.name} is done`);
                    }
                    s.add(next.value);
                }
                yield {
                    id: createId(),
                    generator: this.constructor.name,
                    type: 'class',
                    instance: s,
                };
            }
        }
    }
}

//  TODO: IntersectionGenerator;
//  intersections are just objects

class IntersectionGenerator implements ValueGenerator {
    constructor(
        private generators: Generator<GeneratedParameter, any, any>[],
    ) { }

    *generate(): Generator<GeneratedParameter, any, any> {
        while (true) {
            // const values: any[] = [];
            // const keyCount:Record<string, number> = {};
            // for (const generator of this.generators) {
            //     const next = generator.next();
            //     if (next.done) {
            //         throw new Error(`Generator ${generator.constructor.name} is done`);
            //     }
            //     const o = next.value;
            //     for (const key of Object.keys(o)) {
            //         if (!keyCount[key]) {
            //             keyCount[key] = 0;
            //         }
            //         keyCount[key]++;
            //     }
            //     values.push(o);
            // }

            // const keys = Object.keys(keyCount).filter(k => keyCount[k] === values.length);
            // const o: Record<string, any> = {};
            // for (const key of keys) {
            //     const generatorsForKey = this.generators.map(g => g[key]);
            //     o[key] = new IntersectionGenerator(generatorsForKey).next().value;
            // }


            // yield {
            //     id: createId(),
            //     generator: this.constructor.name,
            //     type: 'object',
            //     properties: values,
            // };
        }
    }
}

class UnionGenerator implements ValueGenerator {
    constructor(
        private generators: Generator<GeneratedParameter, any, any>[],
        private picker: ElementPicker,
    ) { }

    *generate(): Generator<GeneratedParameter, any, any> {
        while (true) {
            for (const index of this.picker(this.generators.length)) {
                const generator = this.generators[index];
                const next = generator.next();
                if (next.done) {
                    throw new Error(`Generator ${generator.constructor.name} is done`);
                }
                yield next.value;
            }
        }
    }
}

//  like an array but can be heterogeneous
class TupleGenerator implements ValueGenerator {
    constructor(
        private generators: Generator<GeneratedParameter, any, any>[],
    ) { }

    *generate(): Generator<GeneratedParameter, any, any> {
        while (true) {
            const values: any[] = [];
            for (let i = 0; i < this.generators.length; i++) {
                const generator = this.generators[i];
                const next = generator.next();
                if (next.done) {
                    throw new Error(`Generator[${i}] ${generator.constructor.name} is done`);
                }
                values.push(next.value);
            }
            yield {
                id: createId(),
                generator: this.constructor.name,
                type: 'array',
                range: values,
            };
        }
    }
}

const targetCollectionSizes = [3, 0, 1, 10, 2];
function* stupidSizer() {
    let i = 0;
    while (true) {
        yield targetCollectionSizes[i++ % targetCollectionSizes.length];
    }
}

function* stupidPicker(max: number) {
    let i = 0;
    while (true) {
        yield i++ % max;
    }
}

function* stupidPropertyPicker(keys: string[]) {
    while (true) {
        yield keys;
    }
}

//  TODO: at some point create jq-compatible paths in pathToHere for neatness
function generatorator(checker: ts.TypeChecker, currentType: ts.Type, currentDepth: number, pathToHere: string[] = []): ValueGenerator {
    
    if (currentDepth < 0) {
        return new LiteralValueGenerator(undefined);
    }

    if (currentType.isLiteral()) {
        return new LiteralValueGenerator(currentType.value);
    }

    if (isEnumType(currentType)) {
        const enumValues = currentType.symbol.members;
        if (enumValues) {
            return new RoundRobinValueGenerator(Object.values(enumValues));
        }
        throw new Error(`Enum type ${checker.typeToString(currentType)} has no values`);
    }

    if (checker.isArrayType(currentType)) {
        const elementType = checker.getTypeArguments(currentType as ts.TypeReference)[0];
        const elementGenerator = generatorator(checker, elementType, currentDepth - 1, pathToHere.concat(".[]"));
        return new ArrayGenerator(elementGenerator.generate(), stupidSizer);
    }
    if (currentType.isIntersection()) {
        const intersectingTypes = currentType.types;
        //  presumably must be an object type
        throw new Error("Not ready for intersectionality");
    }
    if (currentType.isUnion()) {
        const unionTypes = currentType.types;
        const generators: Generator<GeneratedParameter, any, any>[] = [];
        for (const unionType of unionTypes) {
            const g = generatorator(checker, unionType, currentDepth, pathToHere.concat(" | "));
            generators.push(g.generate());
        }
        return new UnionGenerator(generators, stupidPicker);
    }

    switch (currentType.flags) {
        case ts.TypeFlags.Any:
        case ts.TypeFlags.Unknown:
        case ts.TypeFlags.String:
        case ts.TypeFlags.StringLike:
        case ts.TypeFlags.Number:
        case ts.TypeFlags.NumberLike:
        case ts.TypeFlags.Boolean:
        case ts.TypeFlags.BooleanLike:
            // console.log(`Simple ${checker.typeToString(currentType)}`);
            return new SimpleValueGenerator(currentType.flags);

        case ts.TypeFlags.Null:
            // console.log(`Null ${checker.typeToString(currentType)}`);
            return new LiteralValueGenerator(null);

        case ts.TypeFlags.Undefined:
            // console.log(`Undefined ${checker.typeToString(currentType)}`);
            return new LiteralValueGenerator(undefined);

        case ts.TypeFlags.BooleanLiteral:
            checker.getBooleanType();

        case ts.TypeFlags.Object:

            const typeName = currentType.getSymbol()?.getName();
            if (typeName === 'Map') {
                if (!isTypeReference(currentType)) {
                    throw new Error(`Unexpected type not a reference ${checker.typeToString(currentType)}`);
                }
                const [keyType, valueType] = (() => {
                    if (currentType.typeArguments && currentType.typeArguments.length === 2) {
                        return currentType.typeArguments;
                    }
                    //  when types are not specified, just go string=>string
                    return [checker.getStringType(), checker.getStringType()];
                })();

                const keyGenerator = generatorator(checker, keyType, currentDepth - 1, pathToHere.concat('.key'));
                const valueGenerator = generatorator(checker, valueType, currentDepth - 1, pathToHere.concat('.value'));
                // console.log(`Map ${checker.typeToString(currentType)}`)
                return new MapGenerator(keyGenerator.generate(), valueGenerator.generate(), stupidSizer);
            }

            if (typeName === 'Set') {
                if (!isTypeReference(currentType)) {
                    throw new Error(`Unexpected type not a reference ${checker.typeToString(currentType)}`);
                }

                //  when unspecified make it a string
                const elementType = currentType.typeArguments?.length === 1 ? currentType.typeArguments[0] : checker.getStringType();

                const elementGenerator = generatorator(checker, elementType, currentDepth - 1, pathToHere.concat('.element'));
                return new SetGenerator(elementGenerator.generate(), stupidSizer);
            }

            const propertyGenerators: Record<string, Generator<GeneratedParameter, any, any>> = {};
            checker.getPropertiesOfType(currentType).forEach(p => {
                if (p.valueDeclaration) {
                    const t = checker.getTypeOfSymbolAtLocation(p, p.valueDeclaration);
                    propertyGenerators[p.name] = generatorator(checker, t, currentDepth - 1, pathToHere.concat(`.${p.name}`)).generate();
                }
            });

            // console.log(`Object ${checker.typeToString(currentType)}`)
            return new SimpleObjectGenerator(propertyGenerators, stupidPropertyPicker);
    };

    throw new Error(`Unexpected type ${checker.typeToString(currentType)} ${checker.typeToTypeNode(currentType, undefined, undefined)?.getText()}`);
}

//  construct a stateful hierarchy of generators    
function* functionGeneratorator(checker: ts.TypeChecker, f: ts.FunctionDeclaration) {
    const generators: Generator<GeneratedParameter, any, any>[] = [];
    for (let j = 0; j < f.parameters.length; j++) {
        const t = f.parameters[j].type;
        const currentType = t
            ? checker.getTypeAtLocation(t)
            : checker.getAnyType();

        const generator = generatorator(checker, currentType, 3, []).generate();
        generators.push(generator);
    }

    const tg = new TupleGenerator(generators);
    yield* tg.generate();
}

export class CombinatorialTestCaseSource /* implements TestCaseSource */ {
    private counter = 0;

    //  TODO: use this
    private weirdness = 1;

    //  how deep to go into nested objects; meant to be increased
    //  as more parameters are created
    private maxDepth = 3;

    private allExecutedLines = new Set<number>();

    private activeGenerators = new Map<string, Generator<GeneratedParameter, any, any>>();

    constructor(
        //  Have one single handler; if multiple are required, use delegation.  This 
        private checker: ts.TypeChecker,
        private allInstrumentedLines: Set<number>,
        private f: ts.FunctionDeclaration) {
    }

    *seed(): Iterator<Specimen> {
        const newGenPerPass = 10;
        const that = this;
        const f = this.f;
        const checker = this.checker;

        //  TODO: using TupleGenerator and then unpacking like this... needlessly elaborate?
        const toValue = (node: GeneratedParameter): any => {
            if (node.type === 'value') {
                return node.value;
            }
            if (node.type === 'array') {
                return node.range.map(toValue);
            }
            if (node.type === 'object') {
                const o: Record<string, any> = {};
                Object.entries(node.properties).forEach(([k, v]) => {
                    o[k] = toValue(v);
                });
                return o;
            }
            if (node.type === 'class') {
                return node.instance;
            }
            throw new Error(`Unexpected type ${node['type']}`);
        };

        const generator = functionGeneratorator(checker, f);
        for (const value of generator) {
            yield {
                id: createId(),
                sequence: this.counter++,
                parameters: toValue(value),
                type: 'seed',
            };
        }
    }

    increaseWeirdness(): void {
        this.weirdness++;
    }
}