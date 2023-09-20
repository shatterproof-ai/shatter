import * as ts from 'typescript';

import { createId } from "@paralleldrive/cuid2";

import { faker, hy, ne } from '@faker-js/faker';
import { RunResult } from '../core/supervisor';
import { ResultCluster } from '../core/shatter';
import { hybridize } from './hybridize';
import path = require('path');

function* edgyNumbers(m = 1) {
    const primes = [13, 17, 23, 37, 53, 67, 79, 89, 97];

    const neighbors = [-2, -1, 0, 1, 2];
    function* geneighbor(n: number) {
        for (const neighbor of neighbors) {
            const v = n * neighbor;
            yield v;
        }
    }

    const bases = [[2, 63], [5, 6], [10, 10]]
    const mults = [1, -1];

    for (let i = -1; i < 50; i++) {
        const v = m * i;
        yield v;
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

    for (let i = 11; i < 2 ** 32; i = Math.ceil(1.3 * i) + 13) {
        //  utterly stupid; just to make sure it doesn't run out of numbers
        yield i;
        yield -i;
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


    for (const powers of [1, 2, 3]) {
        for (let i = -1; i < 50; i++) {
            for (const seed of seeds) {
                const v = i * seed;
                yield v;
            }
        }

        //  pure exponents e.g. 625, 4096, 100_000_000
        for (const mult of mults) {
            for (const [base, maxponent] of bases) {
                for (let i = 0; i < maxponent; i++) {
                    for (const seed of seeds) {
                        const powered = seed * mult * (base ** i);
                        yield powered;
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
                                yield ppow;
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
            yield i * s;
            yield -(i * s);
        }
    }

}

function* edgyBooleans() {
    while (true) {
        yield true;
        yield false;
    }
}

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

function* edgyAny() {
    while (true) {
        yield {};
    }
}

function* edgyStrings() {
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
    let i = 1;
    let generated = 0;
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
        yield v;
        generated++;
    }
    console.error(`Apparently there are no strings left with i = ${i}; generated = ${generated}`);
}

//  TODO: generify value
export interface GeneratedParameter {
    id: string,
    generator: string,
    value: any
}

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

    private clusterMap = new Map<string, ResultCluster>();

    //  TODO: use this
    private weirdness = 1;

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
    }

    *start(): Iterator<GeneratedParameterList> {
        const newGen = 10;
        const minPerPath = 5;
        const bisectionLimit = 10;
        const mutationLimit = 10;

        const anySeeder = edgyAny();
        const stringSeeder = edgyStrings();
        const numberSeeder = edgyNumbers();
        const floatSeeder = edgyFloats();
        const booleanSeeder = edgyBooleans();

        const edgies: Partial<Record<ts.TypeFlags, Generator[]>> = {
            [ts.TypeFlags.Any]: [anySeeder],
            [ts.TypeFlags.Unknown]: [anySeeder],
            [ts.TypeFlags.String]: [stringSeeder],
            [ts.TypeFlags.Number]: [numberSeeder, floatSeeder],
            [ts.TypeFlags.Boolean]: [booleanSeeder],
        }

        //  TODO: allow reusing a particular value if it's being used in a different place

        const fqseen = new Set<string>();
        const seenStrung = new Set<string>();

        const toKey = (path: string[], value: any) => {
            return JSON.stringify({ path, value })
        }

        const valueForType = function (checker: ts.TypeChecker, currentType: ts.Type, allowedDepth: number, pathToHere: string[],): any {
            if (checker.isArrayType(currentType)) {
                const typeargs = checker.getTypeArguments(currentType as ts.TypeReference);
                const elementttype = typeargs[0];

                return valueForType(checker, elementttype, allowedDepth - 1, pathToHere.concat("[]"));
            }

            if (currentType.flags == ts.TypeFlags.Object) {
                if (allowedDepth === 0) {
                    return null;
                }
                const o: any = {};
                currentType.getProperties().forEach((prop) => {
                    if (prop.valueDeclaration) {
                        const proptype = checker.getTypeOfSymbolAtLocation(prop, prop.valueDeclaration);
                        //  TODO: if the type doesn't allow null or missing....?
                        o[prop.name] = valueForType(checker, proptype, allowedDepth - 1, pathToHere.concat(`["${prop.escapedName}"]`));
                    }
                });

                return o;
            }

            const gengens = edgies[currentType.flags];
            if (!gengens) {
                throw new Error(`Dunno how to handle type ${currentType.flags}`)
            }

            if (gengens.length == 0) {
                throw new Error("Where mah gengens?");
            }

            for (let i = 0; i < gengens.length; i++) {
                let next = gengens[i].next();
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
                console.log(`Unexpectedly done with ${gengens.length} generators`);
            }

            throw new Error(`Ran out of values for ${currentType.flags} and ${JSON.stringify(pathToHere)}`);
        };

        const constructValueForTypeNode = (checker: ts.TypeChecker, typeNode: ts.TypeNode) => {
            const currentType = checker.getTypeAtLocation(typeNode);
            return valueForType(checker, currentType, 4, []);
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
                            const strung = JSON.stringify(hybrid);
                            if (!seenStrung.has(strung)) {
                                yield {
                                    id: createId(),
                                    sequence: this.counter++,
                                    parameters: (hybrid as any[]),
                                };
                                seenStrung.add(strung);
                            }
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
                    if (firstExecutedAfter === undefined) {
                        //  apparently we executed nothing from there to the end
                        //  find the values that got to lastBeforeFirstExecuted and mutate those
                    } else {
                        //  there's a hole in the middle dear liza dear liza
                        const gotToFirstOnly: ResultCluster[] = [];
                        const gotToBoth: ResultCluster[] = [];

                        //  find the values that got to lastBeforeFirstExecuted but not firstExecutedAfter and mutate those
                    }
                }
            }

            for (let i = 0; i < newGen; i++) {
                const parameters: any[] = []
                for (let j = 0; j < this.f.parameters.length; j++) {
                    const t = this.f.parameters[j].type;
                    const p = t ? constructValueForTypeNode(this.checker, t) : 0;
                    parameters.push(p);
                }

                yield {
                    id: createId(),
                    sequence: this.counter++,
                    parameters,
                };
            }
        }
    }

    increaseWeirdness(): void {
        this.weirdness++;
    }

    update(clusterMap: Map<string, ResultCluster>, r: RunResult): void {
        this.clusterMap = clusterMap;
        r.lines.forEach(l => this.allExecutedLines.add(l));
    }
}