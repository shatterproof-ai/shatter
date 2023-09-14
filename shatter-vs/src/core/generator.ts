import * as ts from 'typescript';

import { createId } from "@paralleldrive/cuid2";

import { faker } from '@faker-js/faker';
import { RunResult } from '../core/supervisor';
import { ResultCluster } from '../core/shatter';

const edgyNumbers = () => {
    const numbers = new Set<number>();
    for (let i = -1; i < 50; i++) {
        numbers.add(i);
    }

    for (let i = 0; i < 63; i++) {
        const powered = 2 ** i;
        numbers.add(powered);
        numbers.add(-powered);
    }

    for (let i = 1; i < 10 ** 10; i *= 10) {
        numbers.add(i);
        numbers.add(-i);
    }

    return [...numbers].sort();
};

const edgyBooleans = [true, false];

const numberFakerses = {
    'location': ['latitude', 'longitude']
};

const stringFakerses = {
    'color': ['rgb'],
    'company': ['buzzPhrase'],
    'database': ['type'],
    'finance': ['accountNumber', 'bitcoinAddress', 'creditCardNumber', 'currencyCode', 'currencySymbol', 'iban', 'routingNumber'],
    'git': ['commitSha'],
    'internet': ['domainName', 'domainSuffix', 'httpMethod', /* 'httpStatusCode', */ 'ipv4', 'ipv6', 'mac', 'password', /* 'port',*/ 'protocol', 'url', 'userAgent'],
    'location': ['buildingNumber', 'city', 'country', 'countryCode', 'county', 'direction', 'secondaryAddress', 'state', 'street', 'streetAddress', 'timeZone', 'zipCode'],
    'lorem': ['paragraphs'],
    'person': ['gender', 'jobDescriptor', 'prefix', 'sex', 'suffix', 'zodiacSign'],
    'phone': ['imei', 'number'],
    // 'science': ['chemicalElement', 'unit'],  //  these return objects not strings
    'string': ['uuid'],
    'system': ['cron', 'directoryPath', 'mimeType', 'networkInterface', 'semver'],
    'vehicle': ['fuel', 'manufacturer', 'vin'],
};

// eslint-disable-next-line @typescript-eslint/ban-types
const dataDomains: Record<'string' | 'date' | 'number', Record<string, Function[]>> = {
    string: {},
    number: {
        edgy: [edgyNumbers],
    },
    date: {
        date: [faker.date.past, faker.date.recent, faker.date.soon, faker.date.future]
    }
};

Object.entries(stringFakerses).forEach(([domain, generators]) => {
    generators.forEach(generator => {
        faker[domain as keyof typeof faker];
        const fd = faker[domain as keyof typeof faker];
        const f = [fd[generator as keyof typeof fd]];
        if (!f) {
            throw new Error(`No faker for ${domain}.${generator}`);
        }
        dataDomains.string[`${domain}-${generator}`] = f;
    });
});

faker.seed(10481);

export function seedStrings(count = 1) {
    const seen = new Set<string>();
    const seeds: string[] = [];
    for (let i = 0; i < count; i++) {
        Object.entries(dataDomains.string).forEach(([name, generators]) => {
            if (!['internet-url', 'phone-imei', 'system-cron', 'git-commitSha', 'database-type'].includes(name)) {
                // return
            }
            generators.forEach(generator => {
                const s: string = generator();
                if (!s) {
                    throw new Error(`No seed for ${name}.${generator}`);
                }
                if (typeof s !== 'string') {
                    throw new Error(`Seed for ${name}.${generator} is not a string`);
                }
                if (seen.has(s)) {
                    return;
                }
                seeds.push(s);
            });
        });
    }
    return Array.from(seeds).sort((a, b) => a.localeCompare(b));
}

//  TODO: generify value
export interface GeneratedParameter {
    id: string,
    generator: string,
    value: any
}

export function seedIntegers() {
    //  create an array with all prime numbers less than 100
    const numbers = new Set<number>([13, 17, 23, 37, 53, 67, 79, 89, 97]);

    for (let i = -1; i < 12; i++) {
        numbers.add(i);
    }

    const max = 10 ** 10;
    //  powers of key numbers, their neighbors, and their negatives
    [2, 5, 10].forEach(base => {
        for (let i = base; i < max; i *= base) {
            for (let j = -2; j < 3; j++) {
                const v = i + j;
                numbers.add(v);
                numbers.add(-v);
            }
        }
    });

    return Array.from(numbers).sort();
}

export function seedFloats() {
    const specials = [Math.E, Math.LN10, Math.LN2, Math.LOG10E, Math.LOG2E, Math.PI, Math.SQRT1_2, Math.SQRT2];
    const numbers = new Set<number>([0]);
    for (let i = 0.01; i < 1000; i *= 10) {
        for (const s of specials) {
            numbers.add(i * s);
            numbers.add(-i * s);
        }
    }
    return Array.from(numbers).sort();
}

export interface GeneratedParameterList {
    id: string,
    sequence: number
    parameters: any[]
}

export type EmptyPossibility = {
    type: 'empty'
};

export type PrimitivePossibility = {
    type: 'number' | 'string' | 'boolean'
    range: (null | undefined | number)[]
    | (null | undefined | string)[]
    | (null | undefined | boolean)[]
};

export type ArrayPossibility = {
    type: 'array'
    range: Possibility | null
};

export type ObjectPossibility = {
    type: 'object'
    ranges: Record<string | number, Possibility>
};

export type AnyOrUnknownPossibility = {
    type: 'any' | 'unknown'
    range: any
};

export type Possibility = (EmptyPossibility | PrimitivePossibility | ArrayPossibility | ObjectPossibility | AnyOrUnknownPossibility) & {
    // id: string,
    // generator: string,
};

const possibilitiesForType = function (checker: ts.TypeChecker, currentType: ts.Type, allowedDepth = 1): Possibility | null {

    if (checker.isArrayType(currentType)) {
        const typeargs = checker.getTypeArguments(currentType as ts.TypeReference);
        const elementttype = typeargs[0];

        const elementPossibility = possibilitiesForType(checker, elementttype, allowedDepth - 1);
        return {
            type: 'array',
            range: elementPossibility
        };
    }

    switch (currentType.flags) {
        case ts.TypeFlags.Any:
        case ts.TypeFlags.Unknown:
            return {
                type: 'any',
                range: [{}]
            };
        case ts.TypeFlags.String: {
            const strings = seedStrings();
            return {
                type: 'string',
                range: strings,
            };
        }

        case ts.TypeFlags.Number: {
            const numbers = [
                ...seedIntegers(),
                ...seedFloats(),
            ];

            return {
                type: 'number',
                range: numbers,
            };
        }
        case ts.TypeFlags.Boolean:
            return {
                type: 'boolean',
                range: [true, false],
            };
        case ts.TypeFlags.Object: {
            if (allowedDepth === 0) {
                return null;
            }
            const o: any = {};
            currentType.getProperties().forEach((prop) => {
                if (prop.valueDeclaration) {
                    const proptype = checker.getTypeOfSymbolAtLocation(prop, prop.valueDeclaration);
                    //  TODO: if the type doesn't allow null or missing....?
                    o[prop.name] = possibilitiesForType(checker, proptype, allowedDepth - 1);
                }
            });

            return o;
        }
    }

    return null;
};

const constructValueForTypeNode = (checker: ts.TypeChecker, typeNode: ts.TypeNode) => {
    const currentType = checker.getTypeAtLocation(typeNode);
    return possibilitiesForType(checker, currentType, 4);
};

//  TODO: sometimes throw in a null or undefined
const constructValue = (possibility: Possibility | null): any => {
    if (possibility === null) {
        return null;
    }
    switch (possibility.type) {
        case 'any':
        case 'unknown':
            return possibility.range[Math.floor(Math.random() * possibility.range.length)];
        case 'array':
            if (possibility.range === null) {
                return [];
            }

            const arrayLength = Math.floor(Math.random() * 10);
            const a: any[] = [];
            for (let i = 0; i < arrayLength; i++) {
                a.push(constructValue(possibility.range));
            }

            return a;
        case 'boolean':
        case 'number':
        case 'string':
            return possibility.range[Math.floor(Math.random() * possibility.range.length)];
        case 'object':
            const o: any = {};
            Object.entries(possibility.ranges).forEach(([key, value]) => {
                o[key] = constructValue(value);
            });
            return o;
    }
};

interface TestCaseSource {
    next(): GeneratedParameterList | undefined;
    update?(clusterMap: Map<string, ResultCluster>, r: RunResult): void;
}

export class RetestCaseSource implements TestCaseSource {
    private clusterIndex = 0;
    private resultIndex = 0;
    private counter = 0;
    constructor(private clusters: ResultCluster[]) { }
    next(): GeneratedParameterList | undefined {
        if (this.clusterIndex >= this.clusters.length) {
            return undefined;
        }

        if (this.resultIndex >= this.clusters[this.clusterIndex].results.length) {
            this.clusterIndex++;
            this.resultIndex = 0;
            return this.next();
        }

        const result = this.clusters[this.clusterIndex].results[this.resultIndex];
        this.resultIndex++;
        //  TODO: should this save GeneratedParameterList instead of the bare parameters any[]?
        return {
            id: createId(),
            sequence: this.counter++,
            parameters: result.parameters,
        };
    }
}

export class CombinatorialTestCaseSource implements TestCaseSource {

    private counter = 0;
    private history = new Map<string, GeneratedParameterList>();

    //  map from the JSON path to a particular part of the argument list to a list of candidate values
    //  use up all the seed values before trying anything different
    private possibilities: (Possibility | null)[] = [];

    private clusterMap = new Map<string, ResultCluster>();

    private buffer: GeneratedParameterList[] = [];

    //  TODO: how to fingerprint a particular parameter list so it doesn't get used again?
    //  stringifying the JSON won't work because of canonicalization, self reference, and non-serializable objects
    //  but maybe that's good enough for now

    constructor(
        private checker: ts.TypeChecker,
        private f: ts.FunctionDeclaration) {

        for (const [i, param] of f.parameters.entries()) {
            let paramName = undefined;
            if (ts.isIdentifier(param.name)) {
                paramName = param.name.text;
            }

            if (param.type) {
                if (ts.isTypeReferenceNode(param.type) || ts.isTypeNode(param.type)) {
                    const value = constructValueForTypeNode(checker, param.type);
                    if (value === null) {
                        this.possibilities.push(null);
                    }
                    this.possibilities.push(value);
                } else {
                    throw new Error(`Unexpected type for ${paramName}: ${param.type}`);
                }
            } else {
                throw new Error(`Unexpected type for ${paramName}: ${param.type}`);
            }
        }
    }

    next(): GeneratedParameterList | undefined {
        if (this.buffer.length > 0) {
            const next = this.buffer.shift()!;
            // this.history.set(next?.id, next);
            return next;
        }

        const minPerPath = 5;
        const bisectionLimit = 10;
        const mutationLimit = 10;
        const newGen = 10;

        //  Do some combination of bisection, mutation, and random generation
        /*
        bisection - find two parameter lists that are very similar to each other but lead to different code paths
            //  for each parameter list in a cluster, find the outermost
            //  optimization: record which parameter lists are NOT near the edges of their cluster to avoid reexamining
            //  for each pair of outermosts across all cluster, bisect
        */

        //  find all code paths that haven't been exercised enough

        //  TODO: how to identify the components of a parameter list that were necessary to get past a particular point?
        //  TODO (one day): instrument the getters and see which are accessed in the evaluation of a condition


        //  random
        const parameters: any[] = this.possibilities.map(constructValue);

        const id = createId();
        const gplist: GeneratedParameterList = {
            id,
            sequence: this.counter++,
            parameters,
        };

        this.buffer.push(gplist);


    }

    update(clusterMap: Map<string, ResultCluster>, r: RunResult): void {
        this.clusterMap = clusterMap;
    }
}

export class CCombinatorialTestCaseSource {

    private counter = 0;
    private history = new Map<string, GeneratedParameterList>();

    //  map from the JSON path to a particular part of the argument list to a list of candidate values
    //  use up all the seed values before trying anything different
    private possibilities: (Possibility | null)[] = [];

    //  TODO: how to fingerprint a particular parameter list so it doesn't get used again?
    //  stringifying the JSON won't work because of canonicalization, self reference, and non-serializable objects
    //  but maybe that's good enough for now

    constructor(
        private checker: ts.TypeChecker,
        private parameterDeclarations: ts.NodeArray<ts.ParameterDeclaration>) {

        const jsonPathBase = '$';
        for (const [i, param] of this.parameterDeclarations.entries()) {
            let paramName = undefined;
            if (ts.isIdentifier(param.name)) {
                paramName = param.name.text;
            }

            if (param.type) {
                if (ts.isTypeReferenceNode(param.type) || ts.isTypeNode(param.type)) {
                    const value = constructValueForTypeNode(checker, param.type);
                    if (value === null) {
                        this.possibilities.push(null);
                    }
                    this.possibilities.push(value);
                } else {
                    throw new Error(`Unexpected type for ${paramName}: ${param.type}`);
                }
            } else {
                throw new Error(`Unexpected type for ${paramName}: ${param.type}`);
            }
        }
    }

    generateRandom(desired = 1): GeneratedParameterList[] {
        const gplists: GeneratedParameterList[] = [];
        for (let i = 0; i < desired; i++) {

            const parameters: any[] = this.possibilities.map(constructValue);

            const id = createId();
            const gplist: GeneratedParameterList = {
                id,
                sequence: this.counter++,
                parameters,
            };

            gplists.push(gplist);
            this.history.set(id, gplist);
        }

        return gplists;
    }

    mutate(parameters: GeneratedParameterList, desired = 1): GeneratedParameterList[] {
        const generated: GeneratedParameterList[] = [];
        for (let i = 0; i < desired; i++) {

            const id = createId();
            const gplist: GeneratedParameterList = {
                id,
                sequence: this.counter++,
                parameters: ['yes'],
            };

            this.history.set(id, gplist);
            generated.push(gplist);
        }
        return generated;
    }

    //  takes two parameter lists and finds midpoints
    bisect(a: GeneratedParameterList, b: GeneratedParameterList) {
        const id = createId();
        const gplist: GeneratedParameterList = {
            id,
            sequence: this.counter++,
            parameters: ['yes'],
        };

        this.history.set(id, gplist);
        return gplist;
    }
}