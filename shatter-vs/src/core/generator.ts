import * as ts from 'typescript';

import { ResultCluster } from '../core/shatter';
import { RunResult } from '../core/supervisor';
import { Literals, edgyAny, edgyBooleans, edgyNumberRanges, edgyNumbers, edgyStrings } from './seed';
import { keys, pick, set } from 'lodash';
import { GeneratedParameter, extractGeneratedParameterValue, newId } from './common';
import { type } from 'os';

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
} | {
    type: 'edgication',
    parents: string[],
});

export type Specimen = BaseSpecimen & {
    id: string,
    sequenceInType: number,
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
            id: newId('retest'),
            sequence: this.counter++,
            parameters: [],
        };
    }
}

//  TODO: an allow list for potentially self-referential types
interface GeneratorConfiguration {
    depthLimit: number;
    weirdness: number;
    literals?: Literals;
}

interface GeneratorState {
    currentDepth: number;
    pathToHere: string[];
}

const isObjectType = (type: ts.Type): type is ts.ObjectType => {
    return (type as ts.ObjectType).objectFlags !== undefined;
};

const isTypeReference = (type: ts.Type): type is ts.TypeReference => {
    return isObjectType(type)
        && ((type.objectFlags & ts.ObjectFlags.Reference) !== 0);
};

const isAnonymousType = (type: ts.Type): boolean => {
    return isObjectType(type)
        && ((type.objectFlags & ts.ObjectFlags.Anonymous) !== 0);
};

const isEnumType = (type: ts.Type): type is ts.EnumType => {
    //  TODO: when will this be Enum and when EnumLiteral?
    return ((type.flags & ts.TypeFlags.Enum) !== 0
        || (type.flags & ts.TypeFlags.EnumLiteral) !== 0);
};

type Sizer = (o?: any) => Generator<number, any, any>;
type PropertyPicker = (k: string[], required: Set<string>) => Generator<string[], any, any>;
type ElementPicker = (max: number) => Generator<number, any, any>;

export type G = Generator<GeneratedParameter, any, any>;
type ValueGenerator = (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type, typeAncestors: ts.Type[]) => G | undefined;

const fixedValueGeneratorFactory = function* (generator: string, value: any): G {
    const id = newId('value');
    while (true) {
        yield {
            id,
            generator,
            type: 'value',
            value,
        };
    }
};

const literalValueGeneratorFactory: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type) {
    if (type.isLiteral()) {
        return fixedValueGeneratorFactory('literalValueGeneratorFactory', type.value);
    }
    //  isLiteral() implementation inexplicably does not cover boolean literals
    //            return !!(this.flags & (128 /* StringLiteral */ | 256 /* NumberLiteral */ | 2048 /* BigIntLiteral */));
    if (type.flags & ts.TypeFlags.BooleanLiteral) {
        const t = checker.getTrueType();
        //  TODO: yuck
        const boolvalue = checker.typeToString(type) === checker.typeToString(t);
        return fixedValueGeneratorFactory('literalValueGeneratorFactory', boolvalue);
    }
    return undefined;
};

const simpleTypeFlags = [
    ts.TypeFlags.Any,
    ts.TypeFlags.Unknown,
    ts.TypeFlags.String,
    ts.TypeFlags.Number,
    ts.TypeFlags.Boolean,
    ts.TypeFlags.StringLike,
    ts.TypeFlags.NumberLike,
    ts.TypeFlags.BooleanLike
];

const simpleValueGeneratorFactory: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type) {
    if (simpleTypeFlags.includes(type.flags)) { //  TODO: is this a bitmask?
        //  I think this wrapping is necessary to keep Javascript from being confused about whether there's a generator here; returning immediately from a generator defined with function* without ever yielding still returns a Generator object
        const g = function* () {
            while (true) {
                switch (type.flags) {
                    case ts.TypeFlags.Any:
                    case ts.TypeFlags.Unknown:
                        yield* edgyAny(configuration.literals);
                        break;
                    case ts.TypeFlags.String:
                        yield* edgyStrings(configuration.literals);
                        break;
                    case ts.TypeFlags.Number:
                        yield* edgyNumbers(configuration.literals);
                        break;
                    case ts.TypeFlags.Boolean:
                        yield* edgyBooleans(configuration.literals);
                        break;
                    default:
                        throw new Error(`Unexpected type ${type.flags}`);
                }
            }
        };
        return g();
    }
    return undefined;
};

const enumValueGeneratorFactory: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type): G | undefined {
    if (isEnumType(type)) {
        const enumValues = type.symbol.members;
        if (enumValues) {
            const g = function* () {
                while (true) {
                    for (const v of enumValues) {
                        const gp: GeneratedParameter = {
                            id: newId('enum'),
                            generator: 'enumValueGeneratorFactory',
                            type: 'value',
                            value: v,
                        };
                        yield gp;
                    }
                }
            };
            return g();
        }
        throw new Error(`Enum type ${checker.typeToString(type)} has no values`);
    }
};

interface TwoPhaseGenerator {
    generateEmpty: (configuration: GeneratorConfiguration, state: GeneratorState) => GeneratedParameter;
    generate: (configuration: GeneratorConfiguration, state: GeneratorState) => G;
};


/*

IF there are any subgenerators that can stay under the limit, pick from those

IF there are no subgenerators that can stay under the limit, get as close to the limit as possible and halt
OR throw an error

replace direct access to generators with a wrapper that knows shortest and longest

TODO: increase max depth with weirdness; increase if too many objects can't be fully generated

*/

const stateAwareGenerator = function* (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type, g: TwoPhaseGenerator, typeAncestors: ts.Type[]) {
    const currentDepth = state.currentDepth;
    while (true) {
        if (state.currentDepth >= configuration.depthLimit) {
            const gp: GeneratedParameter = {
                id: newId('terminal'),
                generator: 'stateAwareGenerator-terminal',
                type: 'terminal',
            };
            yield gp;
            return;
        }
        if (state.currentDepth >= configuration.depthLimit) {
            const gg = g.generateEmpty(configuration, state);
            while (currentDepth >= configuration.depthLimit) {
                yield gg;
            }
        } else {
            const gg = g.generate(configuration, state);
            const operatingWeirdness = configuration.weirdness;
            while (currentDepth <= configuration.depthLimit || operatingWeirdness !== configuration.weirdness) {
                const v = gg.next();
                if (v.done) {
                    throw new Error(`Generator ${gg.constructor.name} is done`);
                }
                yield v.value;
            }
        }
    }
};

const arrayValueGenerator: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type, typeAncestors: ts.Type[]): G | undefined {
    if (!checker.isArrayType(type)) {
        return;
    }

    const elementType = checker.getTypeArguments(type as ts.TypeReference)[0];

    const generateEmpty = (): GeneratedParameter => ({
        id: newId('empty-array'),
        generator: 'arrayValueGenerator',
        type: 'array',
        elements: [],
    });

    const generate = function* (configuration: GeneratorConfiguration, state: GeneratorState): G {
        const newState: GeneratorState = {
            currentDepth: state.currentDepth + 1,
            pathToHere: state.pathToHere.concat(".[]"),
        };

        const sizer = stupidSizer;

        if (elementType.flags & ts.TypeFlags.Number) {
            //  in some cases we don't want to think of arrays as collections
            //  of unrelated elements
            yield* edgyNumberRanges(configuration.literals);
        }

        const elementGenerator = generatorator(configuration, checker, newState, elementType, typeAncestors.concat([type]));
        while (true) {
            for (const count of sizer()) {
                const a = [];
                for (let i = 0; i < count; i++) {
                    const next = elementGenerator.next();
                    if (next.done) {
                        throw new Error(`Generator ${elementGenerator.constructor.name} is done`);
                    }
                    a.push(next.value);
                }

                yield {
                    id: newId('array'),
                    generator: 'arrayValueGenerator',
                    type: 'array',
                    elements: a,
                };
            }
        }
    };

    return stateAwareGenerator(configuration, checker, state, type, {
        generateEmpty: generateEmpty,
        generate,
    }, typeAncestors);
};

//  TODO: IntersectionGenerator;
//  intersections are just objects
const intersectionValueGeneratorFactory: ValueGenerator = (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type, typeAncestors: ts.Type[]) => {
    if (!type.isIntersection()) {
        return undefined;
    }

    const generators: G[] = [];
    for (const subtype of type.types) {
        const newState = {
            currentDepth: state.currentDepth,
            pathToHere: state.pathToHere.concat(" | "),
        };
        const g = generatorator(configuration, checker, newState, subtype, typeAncestors.concat([type]));
        generators.push(g);
    }

    function* g(): G {
        while (true) {
            //  intersecting types are always objects
            const parts: GeneratedParameter[] = [];
            for (const generator of generators) {
                const next = generator.next();
                if (next.done) {
                    throw new Error(`Generator ${generator.constructor.name} is done`);
                }
                const o = next.value;
                parts.push(o);
            }

            const gp: GeneratedParameter = {
                id: newId('intersection'),
                generator: 'intersectionValueGeneratorFactory',
                type: 'intersection',
                parts,
            };
            yield gp;
        }
    }

    return g();
};

const unionValueGeneratorFactory: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type, typeAncestors: ts.Type[]): G | undefined {
    if (!type.isUnion()) {
        return undefined;
    }

    const pathToHere = state.pathToHere.concat(" | ");
    const newTypeAncestors = typeAncestors.concat([type]);

    const depths: SelfReferentiality[] = type.types
        .map(subtype => getTypeDepth(checker, subtype, pathToHere, newTypeAncestors));

    const terminableGenerators: G[] = [];
    for (let i = 0; i < type.types.length; i++) {
        if (depths[i].shortest + state.currentDepth <= configuration.depthLimit) {
            const newState = {
                currentDepth: state.currentDepth,
                pathToHere,
            };
            const g = generatorator(configuration, checker, newState, type.types[i], newTypeAncestors);
            terminableGenerators.push(g);
        }
    }

    if (terminableGenerators.length === 0) {
        throw new Error(`Unexpectedly no generators available at depth ${state.currentDepth} <= ${configuration.depthLimit}: ${depths.map(d => d.shortest).join(', ')}`);
    }

    const g = function* () {
        while (true) {
            for (const generator of terminableGenerators) {
                const next = generator.next();
                if (next.done) {
                    throw new Error(`Generator ${generator.constructor.name} is done`);
                }
                const gp = next.value;
                yield gp;
            }
        }
    };
    return g();
};

//  does NOT validate its argument
const mapValueGeneratorFactory: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type, typeAncestors: ts.Type[]) {
    if (!isTypeReference(type)) {
        isTypeReference(type);
        throw new Error(`Unexpected type not a reference ${checker.typeToString(type)}`);
    }

    const sizer = stupidSizer;

    const generateEmpty = (): GeneratedParameter => ({
        id: newId('empty-map'),
        generator: 'mapValueGenerator',
        type: 'map',
        entries: [],
    });

    const generate = function* (configuration: GeneratorConfiguration, state: GeneratorState): G {
        const [keyType, valueType] = (() => {
            if (type.typeArguments && type.typeArguments.length === 2) {
                return type.typeArguments;
            }
            //  when types are not specified, just go string=>string
            return [checker.getStringType(), checker.getStringType()];
        })();

        const updepth = state.currentDepth + 1;
        const keyGenerator = generatorator(configuration, checker, {
            currentDepth: updepth,
            pathToHere: state.pathToHere.concat('.key'),
        }, keyType, typeAncestors.concat([type]));

        const valueGenerator = generatorator(configuration, checker, {
            currentDepth: updepth,
            pathToHere: state.pathToHere.concat('.value'),
        }, valueType, typeAncestors.concat([type]));

        while (true) {
            for (const count of sizer()) {
                const entries: [GeneratedParameter, GeneratedParameter][] = [];
                for (let i = 0; i < count; i++) {
                    const key = keyGenerator.next();
                    if (key.done) {
                        throw new Error(`Generator ${keyGenerator.constructor.name} is done`);
                    }
                    const value = valueGenerator.next();
                    if (value.done) {
                        throw new Error(`Generator ${valueGenerator.constructor.name} is done`);
                    }
                    entries.push([key.value, value.value]);
                }
                yield {
                    id: newId('map'),
                    generator: 'mapValueGenerator',
                    type: 'map',
                    entries,
                };
            }
        }
    };

    return stateAwareGenerator(configuration, checker, state, type, {
        generateEmpty,
        generate,
    }, typeAncestors);
};

const setValueGeneratorFactory: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type, typeAncestors: ts.Type[]) {
    if (!isTypeReference(type)) {
        throw new Error(`Unexpected type not a reference ${checker.typeToString(type)}`);
    }

    const sizer = stupidSizer;

    const generateEmpty = (): GeneratedParameter => ({
        id: newId('empty-set'),
        generator: 'setValueGenerator',
        type: 'class',
        instance: new Set(),
    });

    const generate = function* (configuration: GeneratorConfiguration, state: GeneratorState): G {
        //  when unspecified make it a string
        const elementType = type.typeArguments?.length === 1 ? type.typeArguments[0] : checker.getStringType();

        const newState = {
            currentDepth: state.currentDepth + 1,
            pathToHere: state.pathToHere.concat('.element'),
        };
        const elementGenerator = generatorator(configuration, checker, newState, elementType, typeAncestors.concat([type]));
        while (true) {

            for (const count of sizer()) {
                const entries: GeneratedParameter[] = [];
                for (let i = 0; i < count; i++) {
                    const next = elementGenerator.next();
                    if (next.done) {
                        throw new Error(`Generator ${elementGenerator.constructor.name} is done`);
                    }
                    entries.push(next.value);
                }
                yield {
                    id: newId('set'),
                    generator: 'setValueGeneratorFactory',
                    type: 'set',
                    entries,
                };
            }
        }
    };

    return stateAwareGenerator(configuration, checker, state, type, {
        generateEmpty,
        generate,
    }, typeAncestors);
};

const dateValueGeneratorFactory: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type, typeAncestors: ts.Type[]) {

    const ms1 = 1;
    const ms10 = 10;
    const ms100 = 100;

    const second = 1000;
    const minute = 60 * 1000;
    const hour = 60 * minute;
    const day = 24 * hour;

    const week = 7 * day;
    const month28 = 28 * day;
    const month29 = 29 * day;
    const month30 = 30 * day;
    const month31 = 31 * day;

    const year = 365 * day;
    const leapYear = 366 * day;

    const decade = 10 * year;

    const quanta = [
        ms1, ms10, ms100, second, minute, hour, day, week, month28, month29, month30, month31, year, leapYear, decade,
    ];

    //  randomly generated one time but meant to be used repeatedly for determinism while having good variation
    const perturbations = [
        0,
        ...quanta,
        -14 * ms100,
        33 * minute + 29 * leapYear,
        -45 * hour + 58 * week + -13 * decade,
        60 * ms1 + -3 * day + 24 * month28 + 52 * decade,
        -49 * ms1 + 17 * ms100 + 56 * second + 50 * leapYear,
        -36 * ms10 + -12 * second + -22 * hour + 8 * month29 + 21 * decade,
        5 * ms10 + 13 * minute + 4 * week + -47 * month28 + 46 * leapYear + -9 * decade,
        -57 * ms1 + -24 * ms100 + 37 * second + 6 * minute + -2 * day + 23 * month30 + 59 * year,
        27 * ms10 + -30 * ms100 + -11 * minute + 15 * hour + -54 * day + 60 * week + 39 * month31 + 18 * leapYear,
        38 * ms1 + 19 * ms10 + 20 * second + -35 * minute + 14 * hour + 44 * week + 51 * month29 + -26 * leapYear,
        31 * ms1 + -48 * ms100 + -7 * second + 43 * minute + 3 * day + -5 * week + 55 * month29 + -10 * leapYear + -1 * decade,
        -28 * ms1 + 53 * ms10 + -59 * ms100 + -42 * minute + 57 * day + -21 * week + -17 * month29,
        35 * ms10 + -8 * second + 25 * hour + -41 * day + -16 * month28 + 22 * leapYear + 49 * decade,
        40 * ms1 + -32 * ms10 + 26 * ms100 + -25 * minute + 34 * hour + -40 * week + 2 * month30,
        42 * ms1 + 41 * ms10 + -6 * second + -20 * hour + -23 * day + 16 * week + -56 * month31 + -15 * leapYear + -31 * decade,
        -19 * ms1 + 28 * ms10 + 7 * ms100 + -53 * second + -4 * minute + 54 * day + 9 * month28 + -58 * year,
        -50 * ms10 + 47 * ms100 + 10 * second + -46 * minute + 36 * hour + -34 * day + 12 * week + 32 * month30 + 48 * year,
        -29 * ms1 + -37 * ms10 + -33 * ms100 + 11 * second + 45 * hour + -27 * day + -39 * week + 37 * month31 + -44 * year,
        30 * ms1 + 1 * ms10 + -18 * second + 21 * minute + -38 * hour + 46 * day + 5 * leapYear,
    ];

    const now = Date.now();
    const baseDatesEpoch = [0, now];

    const neighbors = [
        -2,
        -1,
        0,
        1,
        2,
    ];

    function* g(): G {
        for (const neighborOffset of neighbors) {
            for (const perturbationMultiplier of [0, 1, -1, 2, -2]) {
                for (const perturbation of perturbations) {
                    for (const baseDateEpoch of baseDatesEpoch) {
                        const epochMs = baseDateEpoch + perturbation * perturbationMultiplier + neighborOffset;
                        yield {
                            id: newId('date'),
                            generator: 'dateValueGeneratorFactory',
                            type: 'date',
                            epochMs,
                        };
                    }
                }
            }
        }
    }

    return g();
};

const regexpValueGeneratorFactory: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type, typeAncestors: ts.Type[]) {
    const patterns = [/^$/, /.*/,
        //  from https://blog.robertelder.org/regular-expression-test-cases/
        /^([a-z0-9_\.\-]+)@([\da-z\.\-]+)\.([a-z\.]{2,5})$/,
        /employ(|er|ee|ment|ing|able)/,
        /[a-f0-9]{32}/,
        /[A-Fa-f0-9]{64}/,
        /<tag>[^<]*<\/tag>/,
        /<[\s]*tag[^>]*>[^<]*<[\s]*\/[\s]*tag[\s]*>/,
        /^(https?:\/\/)?([\da-z.\-]+)\.([a-z.]{2,6})([\/\w \.\-]*)*\/?$/,
        /Character Classes/,
        /[]/,
        /[^]/,
        /[.]/,
        /[^.]/,
        /[a-b]/,
        /[a-\w]/,
        /[a-\d]/,
        /[^\Wf]/,
        /[^^]/,
        /[日本国]/,
        /\d\D\s\S\w\W/,
        /[\dabc][\D123][\sabc][\S\t][\w\x00][\Wabc]/,
        /Alternation/,
        /()/,
        /(|)/,
        /(||)/,
        /(|||)/,
        /(a|)/,
        /(|b)/,
        /(a|b)/,
        /|/,
        /Quantifiers/,
        /a*/,
        /a+/,
        /a?/,
        /a*?/,
        /a+?/,
        /a??/,
        /a{5}/,
        /a{5}?/,
        /a{,5}/,
        /a{,5}?/,
        /a{5,}/,
        /a{5,}?/,
        /a{5,7}/,
        /a{5,7}?/,
        /abc+|def+/,
        /ab+c|de+f/,
        /a{4}/,
        /(a*){4}/,
        /(){0,1}/,
        /(){1,2}/,
        /()+/,
        /^(a*)*$/,
        /(a+a+)+b/,
        /aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/,
        /(a+?a+?)+?b/,
        /aaaaaaaaaaaaaaaa/,
        /[bc]*(cd)+/,
        /cbcdcd/,
        /Individual Characters/,
        /0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz/,
        /\$\.\(\)\*\+\?\[\\]\^\{\|\}/,
        /\0\t\n\r\v\f\\/,
        /\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0A\x0B\x0C\x0D\x0E\x0F\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1A\x1B\x1C\x1D\x1E\x1F\x20\x21\x22\x23\x24\x25\x26\x27\x28\x29\x2A\x2B\x2C\x2D\x2E\x2F/,
        /\x30\x31\x32\x33\x34\x35\x36\x37\x38\x39\x3A\x3B\x3C\x3D\x3E\x3F\x40\x41\x42\x43\x44\x45\x46\x47\x48\x49\x4A\x4B\x4C\x4D\x4E\x4F\x50\x51\x52\x53\x54\x55\x56\x57\x58\x59\x5A\x5B\x5C\x5D\x5E\x5F/,
        /\x60\x61\x62\x63\x64\x65\x66\x67\x68\x69\x6A\x6B\x6C\x6D\x6E\x6F\x70\x71\x72\x73\x74\x75\x76\x77\x78\x79\x7A\x7B\x7C\x7D\x7E\x7F\x80\x81\x82\x83\x84\x85\x86\x87\x88\x89\x8A\x8B\x8C\x8D\x8E\x8F/,
        /\x90\x91\x92\x93\x94\x95\x96\x97\x98\x99\x9A\x9B\x9C\x9D\x9E\x9F\xA0\xA1\xA2\xA3\xA4\xA5\xA6\xA7\xA8\xA9\xAA\xAB\xAC\xAD\xAE\xAF\xB0\xB1\xB2\xB3\xB4\xB5\xB6\xB7\xB8\xB9\xBA\xBB\xBC\xBD\xBE\xBF/,
        /\xC0\xC1\xC2\xC3\xC4\xC5\xC6\xC7\xC8\xC9\xCA\xCB\xCC\xCD\xCE\xCF\xD0\xD1\xD2\xD3\xD4\xD5\xD6\xD7\xD8\xD9\xDA\xDB\xDC\xDD\xDE\xDF\xE0\xE1\xE2\xE3\xE4\xE5\xE6\xE7\xE8\xE9\xEA\xEB\xEC\xED\xEE\xEF/,
        /\xC0\xC1\xC2\xC3\xC4\xC5\xC6\xC7\xC8\xC9\xCA\xCB\xCC\xCD\xCE\xCF\xD0\xD1\xD2\xD3\xD4\xD5\xD6\xD7\xD8\xD9\xDA\xDB\xDC\xDD\xDE\xDF\xE0\xE1\xE2\xE3\xE4\xE5\xE6\xE7\xE8\xE9\xEA\xEB\xEC\xED\xEE\xEF/,
        /\xF0\xF1\xF2\xF3\xF4\xF5\xF6\xF7\xF8\xF9\xFA\xFB\xFC\xFD\xFE\xFF/,
        /日本国/,
        /HTML\/Javascript Rendering/,
        /<script>alert('XSS')<\/script>/,
        /<script>alert('XSS')<\/script>/,
        /\";alert('XSS');\/\//,
        /\";alert('XSS');\/\//,
        /<svg\/onload=alert('XSS')>/,
        /<svg\/onload=alert('XSS')>/,
        /"><img src="x:x" onerror="alert(XSS)">/,
        /"><img src="x:x" onerror="alert(XSS)">/,
    ];

    function* g(): G {
        while (true) {
            for (const pattern of patterns) {
                yield {
                    id: newId('pattern'),
                    generator: 'regexpValueGeneratorFactory',
                    type: 'regexp',
                    pattern: pattern.toString(),
                };
            }
        }
    }

    return g();
};

const basicObjectValueGeneratorFactory: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type, typeAncestors: ts.Type[]) {
    //  in theory this can be a parameter in the future
    const picker = stupidPropertyPicker;

    const declaredType = checker.typeToString(type);

    const generate = function* (): G {
        const propertyGenerators: Record<string, G> = {};
        const depths: Record<string, SelfReferentiality> = {};
        
        const required = new Set<string>();
        const keysAllowed = new Set<string>();
        checker.getPropertiesOfType(type).forEach(p => {
            if (p.valueDeclaration) {
                const isRequired = !(p.flags & ts.SymbolFlags.Optional);
                if (isRequired) {
                    required.add(p.name);
                }

                const propertyType = checker.getTypeOfSymbolAtLocation(p, p.valueDeclaration);
                const depth = getTypeDepth(checker, propertyType, state.pathToHere.concat(`.${p.name}`), typeAncestors.concat([type]));
                depths[p.name] = depth;

                if (depth.shortest + state.currentDepth <= configuration.depthLimit) {
                    keysAllowed.add(p.name);
                } else {
                    if (required.has(p.name)) {
                        //  TODO: custom error type
                        //  TODO: this should be caught earlier in preflight
                        throw new Error(`Required property ${p.name} cannot be generated at depth ${state.currentDepth} <= ${configuration.depthLimit}`);
                    }
                }

                const newState = {
                    currentDepth: state.currentDepth + 1,
                    pathToHere: state.pathToHere.concat(`.${p.name}`),
                };

                propertyGenerators[p.name] = generatorator(configuration, checker, newState, propertyType, typeAncestors.concat([type]));
            }
        });

        const keysGenerator = picker(Array.from(keysAllowed), required);

        while (true) {
            for (const keys of keysGenerator) {
                const o: Record<string, GeneratedParameter> = {};
                for (const k of keys) {
                    const key = k as string;
                    if (!keysAllowed.has(key)) {
                        continue;
                    }

                    const next = propertyGenerators[key].next();
                    if (next.done) {
                        throw new Error(`Generator ${key} is done`);
                    }

                    const v = extractGeneratedParameterValue(next.value);
                    if (v === undefined && required.has(key)) {
                        console.log(`Required property ${key} is undefined at depth ${state.currentDepth}`);
                    }

                    o[key] = next.value;
                }

                yield {
                    id: newId('object'),
                    generator: 'basicObjectValueGeneratorFactory',
                    type: 'object',
                    properties: o,
                    required: Array.from(required),
                    declaredType,
                };
            }
        }
    };

    return generate();
};

const functionValueGeneratorFactory: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type, typeAncestors: ts.Type[]) {
    const callSignatures = checker.getSignaturesOfType(type, ts.SignatureKind.Call);

    if (callSignatures.length !== 1) {
        throw new Error(`Don't know what to do with ${callSignatures.length} call signatures`);
    }

    const returnType = callSignatures[0].getReturnType();

    const g = function* (): G {
        while (true) {
            for (const returnValue of generatorator(configuration, checker, state, returnType, typeAncestors.concat([type]))) {
                yield {
                    id: newId('function'),
                    generator: 'functionValueGeneratorFactory',
                    type: 'callable',
                    returnValue,
                };
            }
        }
    };

    return g();
};

const DEFAULT_GLOBALS = {
    // eslint-disable-next-line @typescript-eslint/naming-convention
    "VALUE": [
        "globalThis",
        "Infinity",
        "NaN",
        "undefined",
    ],

    // eslint-disable-next-line @typescript-eslint/naming-convention
    "FUNCTION": [
        "eval",
        "isFinite",
        "isNaN",
        "parseFloat",
        "parseInt",
        "decodeURI",
        "decodeURIComponent",
        "encodeURI",
        "encodeURIComponent",
        "escape ",
        "unescape ",
    ],


    // eslint-disable-next-line @typescript-eslint/naming-convention
    "OBJECT": [
        "Function",
        "Boolean",
        "Symbol",
    ],

    // eslint-disable-next-line @typescript-eslint/naming-convention
    "ERROR": [
        "AggregateError",
        "EvalError",
        "RangeError",
        "ReferenceError",
        "SyntaxError",
        "TypeError",
        "URIError",
        "InternalError",
    ],

    // eslint-disable-next-line @typescript-eslint/naming-convention
    "NUMBER": [
        "BigInt",
        "Math",
        "Date",
    ],

    // eslint-disable-next-line @typescript-eslint/naming-convention
    "TEXT": [
        "String",
        "RegExp",
    ],

    // eslint-disable-next-line @typescript-eslint/naming-convention
    "ARRAY": [
        "Int8Array",
        "Uint8Array",
        "Uint8ClampedArray",
        "Int16Array",
        "Uint16Array",
        "Int32Array",
        "Uint32Array",
        "BigInt64Array",
        "BigUint64Array",
        "Float32Array",
        "Float64Array",
    ],

    // eslint-disable-next-line @typescript-eslint/naming-convention
    "KEYED": [
        "Map",
        "Set",
        "WeakMap",
        "WeakSet",
    ],

    // eslint-disable-next-line @typescript-eslint/naming-convention
    "STRUCTURED": [
        "ArrayBuffer",
        "SharedArrayBuffer",
        "DataView",
        "Atomics",
        "JSON",
    ],

    // eslint-disable-next-line @typescript-eslint/naming-convention
    "MEMORY": [
        "WeakRef",
        "FinalizationRegistry",
    ],

    // eslint-disable-next-line @typescript-eslint/naming-convention
    "CONTROL": [
        "Iterator",
        "AsyncIterator",
        "Promise",
        "GeneratorFunction",
        "AsyncGeneratorFunction",
        "Generator",
        "AsyncGenerator",
        "AsyncFunction",
    ],

    // eslint-disable-next-line @typescript-eslint/naming-convention
    "REFLECTION": [
        "Reflect",
        "Proxy",
    ],

    // eslint-disable-next-line @typescript-eslint/naming-convention
    "INTERNATIONALIZATION": [
        "Intl",
        "Intl.Collator",
        "Intl.DateTimeFormat",
        "Intl.DisplayNames",
        "Intl.DurationFormat",
        "Intl.ListFormat",
        "Intl.Locale",
        "Intl.NumberFormat",
        "Intl.PluralRules",
        "Intl.RelativeTimeFormat",
        "Intl.Segmenter",
    ],
};

//  TODO: this does not work if the user creates a new type with the same name as a default global type
//  Tyescript at least in some cases allows it but the parser resolves to the global one
//  So maybe that's okay?
const isDefaultGlobalType = (checker: ts.TypeChecker, type: ts.Type): boolean => {
    const typeName = type.getSymbol()?.getName();
    if (!typeName) {
        return false;
    }

    for (const [category, members] of Object.entries(DEFAULT_GLOBALS)) {
        if (members.includes(typeName)) {
            return true;
        }
    }

    return false;
};

const objectValueGeneratorFactory: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type, typeAncestors: ts.Type[]) {
    if (!(type.flags & ts.TypeFlags.Object)) {
        return;
    };

    //  TODO: find a better way to detect if the type is a function type
    const callSignatures = checker.getSignaturesOfType(type, ts.SignatureKind.Call);
    if (callSignatures.length > 0) {
        return functionValueGeneratorFactory(configuration, checker, state, type, typeAncestors);
    }

    //  TODO: find a better way to detect if the type is a constructor type
    const constructorSignatures = checker.getSignaturesOfType(type, ts.SignatureKind.Construct);
    if (constructorSignatures.length > 0) {
        throw new Error("Not ready for constructors");
    }

    const typeName = type.getSymbol()?.getName();

    if (isDefaultGlobalType(checker, type)) {
        const tn = checker.typeToString(type);
        const declarations = type.getSymbol()?.getDeclarations()?.map(d => {
            const p: any = pick(d.getSourceFile(), 'amdDependencies', 'moduleName', 'languageVariant', 'isDeclarationFile', 'fileName');
            p.text = d.getSourceFile().text.substring(0, 255);
            return p;
        }
        );

        //  only at runtime does the VM resolve the type to an implementation
        //  tsc only knows where it can find the declaration
        // console.log(`symbol name = ${typeName} for ${tn} from ${JSON.stringify(declarations, null, 2)}}`);
        if (typeName === 'Map') {
            return mapValueGeneratorFactory(configuration, checker, state, type, typeAncestors);
        }

        if (typeName === 'Set') {
            return setValueGeneratorFactory(configuration, checker, state, type, typeAncestors);
        }

        if (typeName === 'Date') {
            return dateValueGeneratorFactory(configuration, checker, state, type, typeAncestors);
        }

        if (typeName === 'RegExp') {
            return regexpValueGeneratorFactory(configuration, checker, state, type, typeAncestors);
        }

        //  TODO: Symbol
        //  https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects

        //  TODO: Promise

        throw new Error(`Not ready for default global type ${typeName}`);
    }

    if (type.isClass()) {   //  only detects user-defined classes

    }

    return basicObjectValueGeneratorFactory(configuration, checker, state, type, typeAncestors);

};

const stupidSizer: Sizer = function* () {
    let i = 0;
    const targetCollectionSizes = [3, 0, 1, 10, 2];
    while (true) {
        yield targetCollectionSizes[i++ % targetCollectionSizes.length];
    }
};

const stupidPicker: ElementPicker = function* (max: number) {
    let i = 0;
    while (true) {
        yield i++ % max;
    }
};

const stupidPropertyPicker: PropertyPicker = function* (keys: string[], required: Set<string>) {
    while (true) {
        yield keys;
    }
};

type TypeID = string | number;

interface SelfReferentiality {
    shortest: number;
    longest: number;
}

/*
object flags        Reference = 4,
object flags        Anonymous = 16,

symbol flags         TypeAlias = 524288,
type flags         Object = 524288,


object flags 524288 = ??? maybe object again?
object flags 524368 = ??? Instantiated & Anonymous
*/


function getTypeArgumentsDepth(type: ts.Type, checker: ts.TypeChecker, pathToHere: string[], seen: ts.Type[], expectedTypeArgs?: number): SelfReferentiality[] {
    if (!isTypeReference(type)) {
        if ('typeArguments' in type && (type as any).typeArguments) {
            console.log(`type ${checker.typeToString(type)} ${pathToHere} has ignored type arguments ${(type as any).typeArguments}`);
        }
        return [{
            shortest: 0,
            longest: 0,
        }];
    }

    if (!type.typeArguments) {
        return [{
            shortest: 0,
            longest: 0,
        }];
    }

    if (expectedTypeArgs !== undefined && type.typeArguments.length !== expectedTypeArgs) {
        throw new Error(`Expected ${expectedTypeArgs} type arguments for ${checker.typeToString(type)} but got ${type.typeArguments.length}`);
    }

    const id: TypeID = (type as any).id;
    const typeSRs = type.typeArguments.map(t => {
        const typeSR = getTypeDepth(checker, t, pathToHere.concat(['<type-argument>']), seen);
        //  TODO: how to tell if a type argument is optional?
        return typeSR;
    });

    return typeSRs;
}

//  Math.min returns Infinity if any argument is undefined or if the argument list is empty
const minZero = (...args: number[]): number => {
    if (args.length === 0) {
        return 0;
    }
    return Math.min(...args.filter(a => a !== undefined));
};

//  Math.max returns -Infinity if any argument is undefined or if the argument list is empty
const maxZero = (...args: number[]): number => {
    if (args.length === 0) {
        return 0;
    }
    return Math.max(...args.filter(a => a !== undefined));
};

//  TODO: this function is a test case!
//  to see if there is any way at all out of the maze, not whether there might be a cycle
export const getTypeDepth = (checker: ts.TypeChecker, type: ts.Type, pathToHere: string[], seen: ts.Type[]): SelfReferentiality => {

    const tts = checker.typeToString(type);
    if (tts === 'Clause') {
        console.log('strang');
    }

    const isSimple = simpleTypeFlags.find(f => (f & type.flags) !== 0);

    if (isSimple) {
        console.log(`simple type ${checker.typeToString(type)} ${pathToHere} is not self-referential`);
        return {
            shortest: 0,
            longest: 0,
        };
    }

    const callables = checker.getSignaturesOfType(type, ts.SignatureKind.Call);
    if (callables.length > 0) {
        // console.log(`callable type ${checker.typeToString(type)} ${pathToHere} is not self-referential`);
        return {
            shortest: 0,
            longest: 0,
        };
    }

    const constructors = checker.getSignaturesOfType(type, ts.SignatureKind.Construct);
    if (constructors.length > 0) {
        // console.log(`constructor type ${checker.typeToString(type)} ${pathToHere} is not self-referential`);
        return {
            shortest: 0,
            longest: 0,
        };
    }


    for (const seenType of seen) {
        if ((seenType as any).id === (type as any).id) {
            return {
                shortest: Infinity,
                longest: Infinity,
            };
        }
    }

    const newSeen = seen.concat([type]);

    if (checker.isArrayType(type)) {
        const typeArgsISR = getTypeArgumentsDepth(type, checker, pathToHere, newSeen, 1)[0];

        console.log(`array type ${checker.typeToString(type)} ${pathToHere} is ${JSON.stringify(typeArgsISR)}`);
        return {
            shortest: 1 + typeArgsISR.shortest,
            longest: 1 + typeArgsISR.longest,
        };
    }

    if (type.isUnion()) {
        const depths = type.types.map(t => getTypeDepth(checker, t, pathToHere.concat((['|'])), newSeen));

        //  Union shortest depth is the shortest of the shortest depths of the subtypes
        //  because we can pick which one we want
        const shortest = minZero(...depths.map(d => d.shortest));
        const longest = minZero(...depths.map(d => d.longest));

        const isr = {
            shortest,
            longest,
        };
        console.log(`union type ${checker.typeToString(type)} ${pathToHere} is ${JSON.stringify(isr)}`);
        return isr;
    }

    if (type.isIntersection()) {
        const depths = type.types.map(t => getTypeDepth(checker, t, pathToHere.concat((['|'])), newSeen));

        //  Intersection shortest depth is the largest of the shortest depths of the subtypes
        //  because we don't get a choice; we have to go to them all
        const shortest = maxZero(...depths.map(d => d.shortest));
        const longest = minZero(...depths.map(d => d.longest));
        const isr = {
            shortest,
            longest,
        };
        console.log(`intersection type ${checker.typeToString(type)} ${pathToHere} is ${JSON.stringify(isr)}`);
        return isr;
    }

    if (isDefaultGlobalType(checker, type)) {
        const typeName = type.getSymbol()?.getName();
        if (typeName === 'Date' || typeName === 'RegExp') {
            console.log(`default global type ${checker.typeToString(type)} ${pathToHere} is not self-referential`);
            return {
                shortest: 0,
                longest: 0,
            };
        }

        if (typeName === 'Map') {
            const typeArgsISR = getTypeArgumentsDepth(type, checker, pathToHere, newSeen, 2);
            const [keySelfReferentiality, valueSelfReferentiality] = typeArgsISR;

            const isr = {
                //  MAX on shortest like in an intersection type; can't pick just one of key or value; we need both
                //  although in practice keys are going to be simple types and always be smaller than value types
                shortest: 1 + Math.max(keySelfReferentiality.shortest, valueSelfReferentiality.shortest),
                longest: 1 + Math.max(keySelfReferentiality.longest, valueSelfReferentiality.longest),
            };

            console.log(`map type ${checker.typeToString(type)} ${pathToHere} is ${JSON.stringify(isr)}`);

            return isr;
        }

        if (typeName === 'Set') {
            const typeArgsISR = getTypeArgumentsDepth(type, checker, pathToHere, newSeen, 1)[0];
            console.log(`set type ${checker.typeToString(type)} ${pathToHere} is ${JSON.stringify(typeArgsISR)}`);

            return {
                shortest: 1 + typeArgsISR.shortest,
                longest: 1 + typeArgsISR.longest,
            };
        }
    }

    if (type.isClassOrInterface()) {
        console.log(`class or interface type ${checker.typeToString(type)} ${pathToHere}`);
    }

    const properties = checker.getPropertiesOfType(type);
    const selves = properties.map(p => {
        if (!p.valueDeclaration) {
            //  TODO: determine when this might happen; 0/0 is the lazy answer
            return {
                shortest: 0,
                longest: 0,
            };
        }
        const propertyType = checker.getTypeOfSymbolAtLocation(p, p.valueDeclaration);
        const propertyDepth = getTypeDepth(checker, propertyType, pathToHere.concat([`.${p.getName()}`]), newSeen);
        const isRequired = !(p.flags & ts.SymbolFlags.Optional);

        //  if the property is required, then it could be partially self-referential and fully self-referential
        if (isRequired) {
            console.log(`Required property ${p.getName()} on ${checker.typeToString(type)} from ${pathToHere} ${JSON.stringify(propertyDepth)}`);
            return propertyDepth;
        }

        //  if the property is optional, then it could be partially self-referential and is not fully self-referential
        const isr = {
            shortest: 0,
            longest: propertyDepth.longest,
        };
        console.log(`Optional property ${p.getName()} from ${pathToHere} ${JSON.stringify(isr)}`);
        return isr;
    });

    const typeArgsDepths = getTypeArgumentsDepth(type, checker, pathToHere, newSeen);

    const shortestTypeArgDepth = minZero(...typeArgsDepths.map(s => s.shortest));
    const shortestPropertyDepth = minZero(...selves.map(s => s.shortest));
    //  MAX on shortest like in an intersection type; can't just pick the type arguments or the property types;
    //  we need to get to the bottom of both
    const shortest = Math.max(shortestTypeArgDepth, shortestPropertyDepth);

    const longestTypeArgDepth = maxZero(...typeArgsDepths.map(s => s.longest));
    const longestPropertyDepth = maxZero(...selves.map(s => s.longest));
    const longest = Math.max(longestTypeArgDepth, longestPropertyDepth);

    const depth = {
        shortest,
        longest,
    };

    console.log(`class or interface type ${checker.typeToString(type)} ${pathToHere} is ${JSON.stringify(depth)}`);

    return depth;

    throw new Error(`Not ready for type ${checker.typeToString(type)}`);
};


//  TODO: at some point create jq-compatible paths in pathToHere for neatness
function generatorator(configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, currentType: ts.Type, typeAncestors: ts.Type[]): G {

    if (state.currentDepth > configuration.depthLimit) {
        return fixedValueGeneratorFactory('generatorator', undefined);
    }

    const factories: ValueGenerator[] = [
        literalValueGeneratorFactory,
        simpleValueGeneratorFactory,
        enumValueGeneratorFactory,
        arrayValueGenerator,
        intersectionValueGeneratorFactory,
        unionValueGeneratorFactory,
        objectValueGeneratorFactory,
    ];

    for (const factory of factories) {
        const generator = factory(configuration, checker, state, currentType, typeAncestors.concat([currentType]));
        if (generator) {
            return generator;
        }
    }

    throw new Error(`Unexpected type ${currentType.flags} ${checker.typeToString(currentType)}`);
}

//  construct a stateful hierarchy of generators    
function* functionGeneratorator(checker: ts.TypeChecker, f: ts.FunctionDeclaration, literals?: Literals): Generator<GeneratedParameter[], any, any> {
    if (f.parameters.length === 0) {
        yield [];
        return;
    }

    const MAXIMUM_DEPTH = 6;
    const minimumDepths: number[] = [];
    for (let j = 0; j < f.parameters.length; j++) {
        const parameter = f.parameters[j];
        const ptypeNode = parameter.type;
        if (!ptypeNode) {
            throw new Error(`Parameter ${j} of ${f.name?.getText()} has no type node`);
        }

        const ptype = checker.getTypeFromTypeNode(ptypeNode);
        const depth = getTypeDepth(checker, ptype, [`[${j}]`], []);
        minimumDepths.push(depth.shortest);
    }

    const lowest = minZero(...minimumDepths);
    const highest = maxZero(...minimumDepths);

    const depthReport = minimumDepths.map((shortest, parameterIndex) => {
        const p = f.parameters[parameterIndex];

        const name = p.name?.getText();

        const typeNode = p.type;
        const parameterType = typeNode
            ? checker.getTypeAtLocation(typeNode)
            : checker.getAnyType();

        return {
            index: parameterIndex,
            name,
            shortest: shortest === Infinity ? 'Infinity' : shortest,
            type: checker.typeToString(parameterType),
        };
    });

    console.log(`Depths ${lowest}-${highest}; max = ${MAXIMUM_DEPTH} - ${JSON.stringify(depthReport)}`);

    for (let depthLimit = lowest; depthLimit < MAXIMUM_DEPTH; depthLimit++) {
        const state: GeneratorState = {
            currentDepth: 0,
            pathToHere: [],
        };

        const configuration: GeneratorConfiguration = {
            depthLimit,
            weirdness: 1,
            literals,
        };

        const generatorsByType = new Map<ts.Type, G>();
        let previousValuesByType: Map<ts.Type, any>[] = [];
        //  don't try to convert this to the factory/generator style because function declarations require
        //  an AST Node not just a type
        // const generators: G[] = [];

        for (let j = 0; j < f.parameters.length; j++) {
            const parameter = f.parameters[j];
            const ptypeNode = parameter.type;
            if (!ptypeNode) {
                throw new Error(`Parameter ${j} of ${f.name?.getText()} has no type node`);
            }
        }

        for (let j = 0; j < f.parameters.length; j++) {
            const t = f.parameters[j].type;
            const currentType = t
                ? checker.getTypeAtLocation(t)
                : checker.getAnyType();

            const typeGenerator = generatorsByType.get(currentType);
            if (!typeGenerator) {
                const generator = generatorator(configuration, checker, state, currentType, []);
                const t = checker.typeToString(currentType);
                generatorsByType.set(currentType, generator);
            }
        }

        while (true) {
            //  generate exactly one (1) value for each parameter type; this guarantees that we sometimes pass in identical values
            const valuesByType = new Map<ts.Type, any>();
            generatorsByType.forEach((generator, type) => {
                const next = generator.next();
                if (next.done) {
                    throw new Error(`Generator for ${checker.typeToString(type)} is done`);
                }
                valuesByType.set(type, next.value);
            });

            const newValues: any[] = [];
            for (let j = 0; j < f.parameters.length; j++) {
                const t = f.parameters[j].type;
                const currentType = t
                    ? checker.getTypeAtLocation(t)
                    : checker.getAnyType();

                const v = valuesByType.get(currentType);
                newValues.push(v);
            }

            yield newValues;

            //  no cross blending lists with past lists if there's only one value
            if (newValues.length === 1) {
                continue;
            }

            //  this ensures when different parameters have identical values
            //  we have lists where the values are equal
            //  and lists where they are different
            //  we both want and don't want test cases where the same value is used for multiple parameters
            //  always blend with the last one and then a deterministic subset of the remaining;
            //  this is where we want generators with high variance between successive values
            // for (let i = previousValuesByType.length - 1; i >= 0; i = i * 0.9 - 2) {
            for (let i = previousValuesByType.length - 1; i >= 0; i = i * 0.9 - 2) {
                for (const mod of [2, 3, 5]) {
                    const values: any[] = [];
                    for (let j = 0; j < f.parameters.length; j++) {
                        const t = f.parameters[j].type;
                        const currentType = t
                            ? checker.getTypeAtLocation(t)
                            : checker.getAnyType();

                        //  use both i and j so that we don't always blend the same parameters
                        if ((i + j) % mod === 0) {
                            values.push(previousValuesByType[i].get(currentType));
                        } else {
                            values.push(newValues[j]);
                        }
                    }

                    yield values;
                }
            }

            previousValuesByType.push(valuesByType);
            //  keep a decent number of past rounds to blend with
            if (previousValuesByType.length > Math.min(5, f.parameters.length)) {
                //  TODO: deterministically vary which one gets dropped
                previousValuesByType = previousValuesByType.slice(1);
            }
        }
    }
}

export class CombinatorialTestCaseSource /* implements TestCaseSource */ {
    private counter = 0;

    //  TODO: use this
    private weirdness = 1;

    constructor(
        //  Have one single handler; if multiple are required, use delegation.  This 
        private checker: ts.TypeChecker,
        private f: ts.FunctionDeclaration) {
    }

    *seed(literals?: Literals): Iterator<BaseSpecimen> {
        const f = this.f;
        const checker = this.checker;

        //  TODO: using TupleGenerator and then unpacking like this... needlessly elaborate?
        const generator = functionGeneratorator(checker, f, literals);
        for (const value of generator) {
            const s: BaseSpecimen = {
                parameters: value,
                type: 'seed',
            };
            yield s;
        }
    }

    increaseWeirdness(): void {
        this.weirdness++;
    }
}