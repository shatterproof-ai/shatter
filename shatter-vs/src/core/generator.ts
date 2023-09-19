import * as ts from 'typescript';

import { createId } from "@paralleldrive/cuid2";

import { faker, ne } from '@faker-js/faker';
import { RunResult } from '../core/supervisor';
import { ResultCluster } from '../core/shatter';
import { hybridize } from './hybridize';

function* edgyNumbers2(m = 1) {
    const primes = [13, 17, 23, 37, 53, 67, 79, 89, 97];

    const neighbors = [-2, -1, 0, 1, 2];
    function* geneighbor(n: number) {
        for (const neighbor of neighbors) {
            const v = n * neighbor;
            if (!seen.has(v)) {
                yield v;
                seen.add(v);
            }
        }
    }

    const bases = [[2, 63], [5, 6], [10, 10]]
    const mults = [1, -1];

    const seen = new Set<number>();
    for (let i = -1; i < 50; i++) {
        const v = m * i;
        yield v;
        seen.add(v);
    }

    //  pure exponents e.g. 625, 4096, 100_000_000
    for (const mult of mults) {
        for (const [base, maxponent] of bases) {
            for (let i = 0; i < maxponent; i++) {
                const powered = m * mult * (base ** i);
                for (const n of geneighbor(powered)) {
                    yield n;
                }
            }
        }
    }

    //  e.g. -45, 720, 250
    for (const mult of mults) {
        for (let pow2 = 1; pow2 < 10; pow2++) {
            for (let pow3 = 1; pow3 < 4; pow3++) {
                for (let pow5 = 1; pow5 < 6; pow5++) {
                    const ppow = m * mult * (2 ** pow2) * (3 ** pow3) * (5 ** pow5);
                    for (const n of geneighbor(ppow)) {
                        yield n;
                    }
                }
            }
        }
    }
}

//  progressively get weirder
function* edgyFloats() {
    const seeds = [
        Math.PI,
        Math.E,
        Math.SQRT2,
        Math.LN10,
        Math.LN2,
        Math.LOG10E,
        Math.LOG2E,
        Math.SQRT1_2,
    ];

    const bases = [[2, 63], [5, 6], [10, 10]]
    const mults = [1, -1];

    const seen = new Set<number>();

    for (const powers of [1, 2, 3]) {
        for (let i = -1; i < 50; i++) {
            for (const seed of seeds) {
                const v = i * seed;
                yield v;
                seen.add(v);
            }
        }

        //  pure exponents e.g. 625, 4096, 100_000_000
        for (const mult of mults) {
            for (const [base, maxponent] of bases) {
                for (let i = 0; i < maxponent; i++) {
                    for (const seed of seeds) {
                        const powered = seed * mult * (base ** i);
                        if (!seen.has(powered)) {
                            yield powered;
                            seen.add(powered);
                        }
                    }
                }
            }
        }

        //  e.g. likely fractions
        for (const mult of mults) {
            for (let pow2 = -3; pow2 < 4; pow2++) {
                for (let pow3 = -3; pow3 < 3; pow3++) {
                    for (let pow5 = -3; pow5 < 3; pow5++) {
                        for (let pow7 = -3; pow5 < 3; pow7++) {
                            for (const seed of seeds) {
                                const ppow = seed * mult * (2 ** pow2) * (3 ** pow3) * (5 ** pow5) * (7 ** pow7);
                                if (!seen.has(ppow)) {
                                    yield ppow;
                                    seen.add(ppow);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

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

function* edgyStrings() {
    const seen = new Set<string>();

    const gengen: {
        category: string,
        generator: string,
        function: Function,
    }[] = [];
    for (const [name, generators] of Object.entries(dataDomains.string)) {
        for (const generator of generators) {
            gengen.push({ category: name, generator: generator.name, function: generator })
        };
    }

    let pos = 0;
    for (let i = 1; i < 100_000; i *= 3) {
        const pieces: string[] = [];
        while (pieces.length < i) {
            const v = gengen[i].function();
            pieces.push();
        }
        const v = pieces.join(' ');
        if (!seen.has(v)) {
            yield v;
            seen.add(v);
        }
    }
}

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
    next(): Iterator<GeneratedParameterList>;
    update?(clusterMap: Map<string, ResultCluster>, r: RunResult): void;
}

export class RetestCaseSource implements TestCaseSource {
    private clusterIndex = 0;
    private resultIndex = 0;
    private counter = 0;
    constructor(private clusters: ResultCluster[]) { }
    *next(): Iterator<GeneratedParameterList> {

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

function* crossProductGenerator(input: Possibility[]): Generator<GeneratedParameterList, void, void> {
    if (input.length === 0) {
        return;
    }

    let sequence = 0;
    const [first, ...rest] = input;

    if (first.type === 'empty') {
        for (const node of crossProductGenerator(rest)) {
            yield {
                id: '',
                sequence: sequence++,
                parameters: [undefined, node.parameters]
            };
        }
    } else if (first.type === 'any' || first.type === 'unknown') {
        for (const value of first.range) {
            for (const node of crossProductGenerator(rest)) {
                yield {
                    id: '',
                    sequence: sequence++,
                    parameters: [value, ...node.parameters]
                };
            }
        }
    } else if (first.type === 'number' || first.type === 'string' || first.type === 'boolean') {
        for (const value of first.range) {
            yield {
                id: '',
                sequence: sequence++,
                parameters: [value],
            };
        }
    } else if (first.type === 'array') {
        if (first.range === null) {
            //  TODO: error
            return;
        }

        const lengths = [0, 1, 2, 3, 5, 8, 13];
        const subCrossProductGenerator = crossProductGenerator([first.range]);
        const values: any[] = [];
        const maxNeeded = lengths.reduce((a, b) => a + b, 0);
        for (const subNode of subCrossProductGenerator) {
            values.push(subNode);
            if (values.length >= maxNeeded) {
                break;
            }
        }
        let vi = 0;
        for (const length of lengths) {
            for (let j = 0; j < length; j++) {
                const generated: GeneratedParameterList = {
                    id: '',
                    sequence: sequence++,
                    parameters: values[vi++].parameters,
                };

                yield generated;
                if (vi >= values.length) {
                    vi = 0;
                }
            }
        }
    } else if (first.type === 'object') {
        const keys = Object.keys(first.ranges);
        const bs = Math.floor(Math.random() * 2 ** (keys.length + 1));    //  +1 because we want the leading digit to have a 50% chance
        const permutations = 2 * keys.length;   //  arbitrary; can be smarter later
        for (let i = 0; i < permutations; i++) {
            const values: Record<string, any> = {};
            for (let j = 0; j < keys.length; j++) {
                const key = keys[j];
                const value = first.ranges[key];
                if ((bs & (1 << j)) !== 0) {
                    values[key] = constructValue(value);
                }
            }
            yield {
                id: '',
                sequence: sequence++,
                parameters: [values],
            };
        }
    } else {
        for (const node of crossProductGenerator(rest)) {
            yield node;
        }
    }
}

const comparameters = (a: any, b: any): number => {
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



export class CombinatorialTestCaseSource implements TestCaseSource {

    private counter = 0;
    private history = new Map<string, GeneratedParameterList>();

    //  map from the JSON path to a particular part of the argument list to a list of candidate values
    //  use up all the seed values before trying anything different
    private possibilities: (Possibility | null)[] = [];

    private clusterMap = new Map<string, ResultCluster>();

    private buffer: GeneratedParameterList[] = [];

    //  how deep to go into nested objects; meant to be increased
    //  as more parameters are created
    private maxDepth = 3;

    private allExecutedLines = new Set<number>();
    //  TODO: how to fingerprint a particular parameter list so it doesn't get used again?
    //  stringifying the JSON won't work because of canonicalization, self reference, and non-serializable objects
    //  but maybe that's good enough for now

    constructor(
        private checker: ts.TypeChecker,
        private allInstrumentedLines: Set<number>,
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

    *next(): Iterator<GeneratedParameterList> {
        if (this.buffer.length > 0) {
            const next = this.buffer.shift()!;
            // this.history.set(next?.id, next);
            yield next;
        }

        const newGen = 10;
        const minPerPath = 5;
        const bisectionLimit = 10;
        const mutationLimit = 10;

        /**
         * 0) sort all the clusters by their highest numbered line 
         * 1) Look through all the clusters
         *  2) for each parameter in the array of parameters
         *      2a) sort each cluster by the value of that parameter
         *      2b) foreach pair of clusters (TODO: can be smarter than every pair), bisect
         */
        const clusters = Array.from(this.clusterMap.values());
        const clusterMaxLines = new Map<string, number>();
        clusters.forEach(c => {
            const max = c.lines.reduce((a, b) => Math.max(a, b), 0);
            clusterMaxLines.set(c.key, max);
        });

        clusters.sort((a, b) => {
            const aMax = clusterMaxLines.get(a.key)!;
            const bMax = clusterMaxLines.get(b.key)!;
            return aMax - bMax;
        });

        /*
        bisection - find two parameter lists that are very similar to each other but lead to different code paths
            //  for each parameter list in a cluster, find the outermost
            //  optimization: record which parameter lists are NOT near the edges of their cluster to avoid reexamining
            //  for each pair of outermosts across all cluster, bisect
        */
        let bisections = 0;
        while (bisections < bisectionLimit) {
            for (let index = 0; index < this.f.parameters.length; index++) {
                for (let i = 0; i < clusters.length - 1; i++) {
                    const a = clusters[i];
                    const b = clusters[i + 1];
                    a.results.sort(comparameters);
                    b.results.sort(comparameters);

                    const alast = a.results[a.results.length - 1];
                    const alastCurrentParam = alast.parameters[index];
                    const bfirst = b.results[0];
                    const bfirstCurrentParam = bfirst.parameters[index];

                    for (const hybrid of hybridize(alastCurrentParam, bfirstCurrentParam)) {
                        yield  {
                            id: createId(),
                            sequence: this.counter++,
                            parameters: (hybrid as any[]),
                        };
                    }
                }
            };

            bisections++;
        }

        /**
         * mutation
         * 1) find lines that have been instrumented but not executed
         * 2) identify clusters that have exercised the lines before and/or after
         * 3) generate parameter lists that are similar to the ones
         *      used to get to the before and different from the after
         * 
         * 
         */

        Array.from(this.allInstrumentedLines).sort();
        for (let i = 0; i < mutationLimit; i++) {
        }



        for (let i = 0; i < newGen; i++) {
            const parameters: any[] = this.possibilities.map(constructValue);

            const id = createId();
            const gplist: GeneratedParameterList = {
                id,
                sequence: this.counter++,
                parameters,
            };

            this.buffer.push(gplist);
            yield gplist;
        }

        //  Do some combination of bisection, mutation, and random generation

        //  find all code paths that haven't been exercised enough

        //  TODO: how to identify the components of a parameter list that were necessary to get past a particular point?
        //  TODO (one day): instrument the getters and see which are accessed in the evaluation of a condition


        //  random
        while (true) {
            const parameters: any[] = this.possibilities.map(constructValue);

            const id = createId();
            const gplist: GeneratedParameterList = {
                id,
                sequence: this.counter++,
                parameters,
            };

            this.buffer.push(gplist);
            yield gplist;
        }


    }

    update(clusterMap: Map<string, ResultCluster>, r: RunResult): void {
        this.clusterMap = clusterMap;
        r.lines.forEach(l => this.allExecutedLines.add(l));
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