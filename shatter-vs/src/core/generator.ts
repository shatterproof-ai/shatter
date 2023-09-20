import * as ts from 'typescript';

import { createId } from "@paralleldrive/cuid2";

import { faker } from '@faker-js/faker';
import { distance as levenshtein } from 'fastest-levenshtein';
import { ResultCluster } from '../core/shatter';
import { RunResult } from '../core/supervisor';
import { hybridize } from './hybridize';
import path = require('path');

const gpv = (value: number | string | boolean, generator: string, options?: Record<string, any>): GeneratedParameter => ({
    id: createId(),
    generator,
    type: 'value',
    value,
    options,
});

const primeSortModBase = 7;
const primes = [11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73, 79, 83, 89, 97].sort((a, b) => (a % primeSortModBase) - (b % primeSortModBase));
function* edgyNumbers(m = 1): Generator<GeneratedParameter, void, unknown> {
    //  stupid sort to avoid favoring small values but still be deterministic

    const neighbors = [-2, -1, 0, 1, 2];

    function* geneighbor(n: number, generator: string) {
        for (const neighbor of neighbors) {
            const v = n * neighbor;
            yield gpv(v, generator);
        }
    }

    const bases = [[2, 63], [5, 6], [10, 10]];
    const mults = [1, -1];

    for (let i = -1; i < 11; i++) {
        const v = m * i;
        yield gpv(v, 'smallWholes');
    }

    for (const prime of primes) {
        const v = m * prime;
        for (const gp of geneighbor(v, 'primes')) {
            yield gp;
        }
    }

    //  pure exponents e.g. 625, 4096, 100_000_000
    for (const mult of mults) {
        for (const [base, maxponent] of bases) {
            for (let i = 0; i < maxponent; i++) {
                const powered = m * mult * (base ** i);
                for (const gp of geneighbor(powered, 'pureExponents')) {
                    yield gp;
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
                    for (const gp of geneighbor(ppow, 'exponentProducts')) {
                        yield gp;
                    }
                }
            }
        }
    }

    for (let i = 11; i < 2 ** 32; i = Math.ceil(1.3 * i) + 13) {
        //  utterly stupid; just to make sure it doesn't run out of numbers
        yield gpv(i, 'positiveStupid');
        yield gpv(-i, 'negativeStupid');
    }
}

//  progressively get weirder
function* edgyFloats(): Generator<GeneratedParameter, void, unknown> {
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

    const bases = [[2, 63], [5, 6], [10, 10]];
    const mults = [1, -1];

    for (const powers of [1, 2, 3]) {
        for (let i = -1; i < 50; i++) {
            for (const seed of seeds) {
                const v = i * seed;
                const generatorName = i === 1 ? 'basicRationals' : 'basicRationalSimpleMultiples';
                yield gpv(v, generatorName);
            }
        }

        //  pure exponents e.g. 625, 4096, 100_000_000
        for (const mult of mults) {
            for (const [base, maxponent] of bases) {
                for (let i = 0; i < maxponent; i++) {
                    for (const seed of seeds) {
                        const powered = seed * mult * (base ** i);
                        yield gpv(powered, 'basicRationalsComplexMultiples');
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
                                yield gpv(ppow, 'basicRationalsExponentialProducts');
                            }
                        }
                    }
                }
            }
        }
    }

    //  utterly stupid; just to make sure it doesn't run out of numbers
    for (let i = 11; i < 2 ** 32; i = Math.ceil(1.3 * i) + 13) {
        for (const s of seeds) {
            yield gpv(i * s, 'basicRationalsPositiveStupid');
            yield gpv(-(i * s), 'basicRationalsNegativeStupid');
        }
    }

}

function* edgyBooleans(): Generator<GeneratedParameter, void, unknown> {
    while (true) {
        yield gpv(true, 'edgyBooleans');
        yield gpv(false, 'edgyBooleans');
    }
}

const numberFakerses = {
    'location': ['latitude', 'longitude']
};

const optionVariants: Record<string, Record<string, any>> = {
    email: {
        allowSpecialCharacters: [true, false],
    },
    mac: {
        separator: [':', '-', ''],
    },
    password: {
        length: [1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 12, 13, 14, 15, 16, 17, 31, 32, 33, 39, 40, 41, 47, 48, 49, 63, 64, 65, 127, 128, 129],
        memorable: [true, false],
    },
    url: {
        appendSlash: [true, false],
        protocol: ['http', 'https'],
    },
    commitSha: {
        length: [8, 16, 32, 40, 64],
    },
    countryCode: {
        variant: ['alpha-2', 'alpha-3', 'numeric'],
    },
    state: {
        abbreviated: [true, false],
    },
    paragraph: {
        sentenceCount: [1, 3, 9, 100, 500, 1111, 9999, 100_000],
    },
    alpha: {
        casing: ['upper', 'lower', 'mixed'],
        length: [1, 3, 7, 15, 32, 33, 99],
    },
    alphanumeric: {
        casing: ['upper', 'lower', 'mixed'],
        length: [1, 3, 7, 15, 32, 33, 99],
    },
    binary: {
        length: [1, 3, 7, 15, 32, 33, 99],
    },
    hexadecimal: {
        casing: ['upper', 'lower', 'mixed'],
        length: [1, 3, 7, 15, 32, 33, 99],
    },
    numeric: {
        allowLeadingZeros: [true, false],
        length: [1, 3, 7, 15, 32, 33, 99],
    },
    octal: {
        length: [1, 3, 7, 15, 32, 33, 99],
    },
    networkInterface: {
        interfaceType: ['en', 'wl', 'ww'],
    },
};

const stringFakerses = {
    'color': ['rgb'],
    'company': ['buzzPhrase'],
    'database': ['type'],
    'finance': ['accountNumber', 'bitcoinAddress', 'creditCardNumber', 'currencyCode', 'currencySymbol', 'iban', 'routingNumber'],
    'git': ['commitSha'],
    'internet': ['domainName', 'domainSuffix', 'emoji', 'httpMethod', /* 'httpStatusCode', */ 'ipv4', 'ipv6', 'mac', 'password', /* 'port',*/ 'protocol', 'url', 'userAgent'],
    'location': ['buildingNumber', 'city', 'country', 'countryCode', 'county', 'direction', 'secondaryAddress', 'state', 'street', 'streetAddress', 'timeZone', 'zipCode'],
    'lorem': ['paragraphs'],
    'person': ['gender', 'jobDescriptor', 'prefix', 'sex', 'suffix', 'zodiacSign'],
    'phone': ['imei', 'number'],
    // 'science': ['chemicalElement', 'unit'],  //  these return objects not strings
    'string': ['alpha', 'alphanumeric', 'binary', 'hexadecimal', 'numeric', 'octal', 'uuid'],
    'system': ['cron', 'directoryPath', 'mimeType', 'networkInterface', 'semver'],
    'vehicle': ['fuel', 'manufacturer', 'vin'],
};

// eslint-disable-next-line @typescript-eslint/ban-types
const dataDomains: Record<'string' | 'date', Record<string, Function[]>> = {
    string: {},
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

function* edgyAny(): Generator<GeneratedParameter, void, unknown> {
    while (true) {
        yield {
            id: createId(),
            generator: 'edgyAny',
            type: 'object',
            properties: {},
        };
    }
}

//  TODO: apply options
function* edgyStrings(): Generator<GeneratedParameter, void, unknown> {
    const gengen: {
        category: string,
        generator: string,
        function: Function,
    }[] = [];
    for (const [name, generators] of Object.entries(dataDomains.string)) {
        for (const generator of generators) {
            gengen.push({ category: name, generator: generator.name, function: generator });
        };
    }

    let pos = 0;
    let i = 1;
    let generated = 0;

    //  variations on just one thing
    for (let i = 0; i < 10; i++) {
        for (const gen of gengen) {
            const v: string = gen.function();
            yield gpv(v, 's(tr)ingle');
            generated++;
        }
    }

    //  a mix of things
    for (; i < 100_000; i = Math.ceil(i * 1.2)) {
        const pieces: string[] = [];
        while (pieces.length < i) {
            if (pos >= gengen.length) {
                pos = 0;
            }
            const v = gengen[pos++].function();
            pieces.push(v);
        }
        const v = pieces.join(' ');
        yield gpv(v, 'mingle');
        generated++;
    }
    console.error(`Apparently there are no strings left with i = ${i}; generated = ${generated}`);
}

//  TODO: generify value
export type GeneratedParameter = {
    id: string,
    generator: string,
    options?: Record<string, any>,
} & ({
    type: 'value',
    value: any,
} | {
    type: 'array',
    range: GeneratedParameter[],
} | {
    type: 'object',
    properties: Record<string, GeneratedParameter>,
});

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

function* crossProductGenerator(input: any[]): Generator<GeneratedParameterList, void, void> {
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
                    // values[key] = constructValue(value);
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

function* roundRobin(...generators: Generator<any, any, any>[]) {
    let i = 0;
    while (true) {
        const g = generators[i];
        const next = g.next();
        if (next.done) {
            generators[i] = generators[i];
        } else {
            yield next.value;
            i = (i + 1) % generators.length;
        }
    }
}

function computeDistance(a: any, b: any): number {
    if (a === b || a === null || b === null) {
        return 0;
    }

    if (typeof a === 'number') {
        const smaller = Math.min(a, b);
        const larger = Math.max(a, b);
        const difference = larger - smaller;
        if (difference == 0) {
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

export class CombinatorialTestCaseSource implements TestCaseSource {

    private counter = 0;
    private history = new Map<string, GeneratedParameterList>();

    //  map from the JSON path to a particular part of the argument list to a list of candidate values
    //  use up all the seed values before trying anything different

    private clusterMap = new Map<string, ResultCluster>();

    //  TODO: use this
    private weirdness = 1;

    //  how deep to go into nested objects; meant to be increased
    //  as more parameters are created
    private maxDepth = 3;

    totalHybrids = 0;
    totalMutations = 0;
    totalSeeds = 0;

    private allExecutedLines = new Set<number>();
    //  TODO: how to fingerprint a particular parameter list so it doesn't get used again?
    //  stringifying the JSON won't work because of canonicalization, self reference, and non-serializable objects
    //  but maybe that's good enough for now

    constructor(
        private checker: ts.TypeChecker,
        private allInstrumentedLines: Set<number>,
        private f: ts.FunctionDeclaration) {
    }


    /*
    1) generate a varied set of inputs
    2) run them
    3) cluster them
    4) foreach value in a cluster, keep minimizing until it's no longer in the cluster 
        (be sure to check to see if the minimized version is already in the cluster)
    5) identify overlooked lines and try to mutate the minima to cover them (how to avoid just regenerating the non-minimal values or ones that will be similarly ineffective?)
    6) take the minima and compare them against the other clusters and hybridize for edginess

    */

    *start(): Iterator<GeneratedParameterList> {
        const newGenPerPass = 10;
        const minPerPath = 5;
        const bisectionLimitPerPass = 10;
        const mutationLimitPerPass = 10;

        const edgies: Partial<Record<ts.TypeFlags, (() => Generator<GeneratedParameter, any, any>)>> = {
            [ts.TypeFlags.Any]: edgyAny,
            [ts.TypeFlags.Unknown]: edgyAny,
            [ts.TypeFlags.String]: edgyStrings,
            [ts.TypeFlags.Number]: () => roundRobin(edgyNumbers(), edgyFloats()),
            [ts.TypeFlags.Boolean]: edgyBooleans,
        };

        //  TODO: allow reusing a particular value if it's being used in a different place
        const valueGenerators = new Map<string, Generator<GeneratedParameter, any, any>>();

        const fqseen = new Set<string>();
        const seenStrung = new Set<string>();

        //  TODO: at some point create jq-compatible paths for neatness
        const toKey = (path: (string | number)[], value: any) => {
            return JSON.stringify({ path, value });
        };

        const valueForType = function (checker: ts.TypeChecker, currentType: ts.Type, allowedDepth: number, pathToHere: (string | number)[],): GeneratedParameter {
            if (checker.isArrayType(currentType)) {
                const typeargs = checker.getTypeArguments(currentType as ts.TypeReference);
                const elementttype = typeargs[0];

                const values: any[] = [];

                const length = Math.floor(Math.random() * 10);

                for (let i = 0; i < length; i++) {
                    const a = valueForType(checker, elementttype, allowedDepth - 1, pathToHere.concat(".[]"));
                    values.push(a);
                }

                return {
                    id: createId(),
                    generator: 'array',
                    type: 'array',
                    range: values,
                    options: {
                        length,
                    },
                };
            }

            if (currentType.flags === ts.TypeFlags.Object) {
                if (allowedDepth === 0) {
                    return {
                        id: createId(),
                        generator: 'object',
                        type: 'object',
                        properties: {},
                    };
                }
                //  TODO: omit some, add some extra
                const o: Record<string, GeneratedParameter> = {};
                currentType.getProperties().forEach((prop) => {
                    if (prop.valueDeclaration) {
                        const proptype = checker.getTypeOfSymbolAtLocation(prop, prop.valueDeclaration);
                        //  TODO: if the type doesn't allow null or missing....?
                        o[prop.name] = valueForType(checker, proptype, allowedDepth - 1, pathToHere.concat(`.["${prop.escapedName}"]`));
                    }
                });

                return {
                    id: createId(),
                    generator: 'object',
                    type: 'object',
                    properties: o,
                };
            }

            const strungPath = pathToHere.join('.');
            let generator = valueGenerators.get(strungPath);
            if (!generator) {
                const gengens = edgies[currentType.flags];
                if (!gengens) {
                    throw new Error(`Dunno how to handle type ${currentType.flags}`);
                }

                generator = gengens();
                valueGenerators.set(strungPath, generator);
            }

            let next = generator.next();
            if (!next.done) {
                const key = toKey(pathToHere, next.value);
                //  in theory we want to avoid the same value in the same place repeatedly
                //  but it's not terrible, and the whole object duplicate avoidance may be adequate
                // if (!fqseen.has(key)) {
                fqseen.add(key);
                return next.value;
                // }
                // next = gengens[i].next();
            }

            throw new Error(`Ran out of values for ${currentType.flags} and ${JSON.stringify(pathToHere)}`);
        };

        while (true) {
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

            //  KNOWN FLAW: these bisections may be obsolete because the clusters only get analyzed once per loop instead of on each generation
            if (clusters.length > 0) {
                let that = this;
                function* bisector() {
                    for (let index = 0; index < that.f.parameters.length; index++) {
                        for (let i = 0; i < clusters.length - 1; i++) {
                            const a = clusters[i];
                            const b = clusters[i + 1];
                            a.results.sort(comparameters);
                            b.results.sort(comparameters);

                            const alast = a.results[a.results.length - 1];
                            const bfirst = b.results[0];

                            const distance = computeDistance(alast.parameters[index], bfirst.parameters[index]);
                            if (distance <= 1) {
                                //  found the edges or close enough
                                console.log(`found edges ${distance} between ${JSON.stringify(alast.parameters[index])} and ${JSON.stringify(bfirst.parameters[index])}`);
                                continue;
                            }
                            console.log(`distance ${distance} between ${JSON.stringify(alast.parameters[index])} and ${JSON.stringify(bfirst.parameters[index])}`);

                            //  generate a parameter list where every parameter is hybridized between alast and bfirst
                            const hybridized = hybridize(alast.parameters, bfirst.parameters);
                            for (const fullHybrid of hybridized) {
                                //  also generate a parameter list based on alast with just the current parameter hybridized
                                const abased = [...alast.parameters];
                                abased[index] = fullHybrid[index];

                                //  also generate a parameter list based on bfirst with just the current parameter hybridized
                                const bbased = [...bfirst.parameters];
                                bbased[index] = fullHybrid[index];

                                for (const hybrid of [fullHybrid, abased, bbased]) {
                                    const strung = JSON.stringify(hybrid);
                                    if (!seenStrung.has(strung)) {
                                        console.log(`hybridized ${strung} from ${JSON.stringify(alast.parameters)} and ${JSON.stringify(bfirst.parameters)})}`);
                                        yield {
                                            id: createId(),
                                            sequence: that.counter++,
                                            parameters: (fullHybrid as any[]),
                                        };
                                        seenStrung.add(hybrid);
                                    }
                                }
                            }
                        }
                    };
                }

                let hybrids = 0;
                for (const bisection of bisector()) {
                    yield bisection;
                    hybrids++;
                    this.totalHybrids++;
                    if (hybrids >= bisectionLimitPerPass) {
                        break;
                    }
                }
            }

            /**
             * Reduction
             * TODO: take parameters that have already been run, trim away some stuff, and run the result
             * trimming = shortening strings, shortening arrays, and removing object properties
             * 
             */

            /**
             * mutation
             * 1) find lines that have been instrumented but not executed
             * 2) identify clusters that have exercised the lines before and/or after
             * 3) generate parameter lists that are similar to the ones
             *      used to get to the before and different from the after
             * 
             * 
             */

            let mutations = 0;
            const allInstrumentedLines = Array.from(this.allInstrumentedLines).sort();
            let lastBeforeFirstExecuted: number | undefined = undefined;
            let firstUnexecuted: number | undefined = undefined;
            let i = 0;
            for (; i < allInstrumentedLines.length; i++) {
                const line = allInstrumentedLines[i];
                if (!this.allExecutedLines.has(line)) {
                    firstUnexecuted = line;
                    break;
                }
                lastBeforeFirstExecuted = line;
            }

            /*
            //  in theory a tree type structure seems like the way to go here,
            //  but (I think) simple line numbers do well enough; if we have some
            //  code that got executed, then some code that didn't, and then optionally
            //  some more code that, we can be pretty confident that the middle part was
            //  in conditional or loop body, and that what got executed later is 
            //  either an explicit else, an implicit else, or just normal unconditional
            //  execution but either way it didn't satisfy the requirements of the missing
            //  part, so we can say we want inputs like what got to the first part but unlike what got
            //  to the third part.
            */
            if (firstUnexecuted !== undefined) {
                let firstExecutedAfter: number | undefined = undefined;
                for (; i < allInstrumentedLines.length; i++) {
                    const line = allInstrumentedLines[i];
                    if (this.allExecutedLines.has(line)) {
                        firstExecutedAfter = line;
                        break;
                    }
                }

                //  if at least one line was executed...
                if (lastBeforeFirstExecuted !== undefined) {
                    //  otherwise Typescript doesn't know that lastBeforeFirstExecuted is defined
                    const lbfe = lastBeforeFirstExecuted;
                    //  should be in order from lowest last line to highest last line
                    //  based on the sorting done before bisection
                    const ranBefore = clusters.filter(c => c.lines.includes(lbfe));
                    const ranAfter = clusters.filter(c => firstExecutedAfter && c.lines.includes(firstExecutedAfter));
                    const ranBeforeOnly = ranBefore.filter(c => !ranAfter.includes(c));

                    if (firstExecutedAfter === undefined) {
                        //  apparently we executed nothing from there to the end
                        //  find the values that got to lastBeforeFirstExecuted and mutate those
                    } else {
                        //  there's a hole in the middle dear liza dear liza
                        const gotToFirstOnly: ResultCluster[] = [];
                        const gotToBoth: ResultCluster[] = [];

                        //  find the values that got to lastBeforeFirstExecuted but not firstExecutedAfter and mutate those
                        //  X = identify what's common about the values that got to firstExecutedAfter
                        //  Y = identify what's common about ALL the values that got to lastBeforeFirstExecuted
                        //  mutate the values of X in a way that is not similar to Y
                    }
                }
            }

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
            };

            for (let i = 0; i < newGenPerPass; i++) {
                const parameters: any[] = [];
                for (let j = 0; j < this.f.parameters.length; j++) {
                    const t = this.f.parameters[j].type;
                    const currentType = t
                        ? this.checker.getTypeAtLocation(t)
                        : this.checker.getAnyType();

                    const p: GeneratedParameter = valueForType(this.checker, currentType, 4, [j]);
                    parameters.push(toValue(p));
                }

                yield {
                    id: createId(),
                    sequence: this.counter++,
                    parameters,
                };
                this.totalSeeds++;
            }
        }
    }

    stats() {
        return {
            totalHybrids: this.totalHybrids,
            totalMutations: this.totalMutations,
            totalSeeds: this.totalSeeds,
        };
    }

    increaseWeirdness(): void {
        this.weirdness++;
    }

    update(clusterMap: Map<string, ResultCluster>, r: RunResult): void {
        this.clusterMap = clusterMap;
        r.lines.forEach(l => this.allExecutedLines.add(l));
    }
}