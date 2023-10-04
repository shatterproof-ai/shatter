// import * as ts from 'typescript';

import { pick } from 'lodash';
import { ResultCluster } from '../core/shatter';
import { RunResult } from '../core/supervisor';
import { GeneratedParameter, ObjectPathSegment, ValueGeneratedParameter, isValueSubtype, mergePath, newId, rehydrateGeneratedParameterValue } from './common';
import { Literals, edgyAny, edgyBooleans, edgyNumberRanges, edgyNumbers, edgyStrings } from './seed';
import ts = require('typescript');

/*

TODO:
given the number of a line of code, find all conditionals before that line that must be passed in order to reach that line.
extract each conditional into a unique variable.
include all assignments to all variables that are incorporated into each conditional.
//  TODO: simplify conditionals to remove redundant variables and clauses
produce a final variable that is the conjunction of all conditionals.
provide the function signature and these conditionals and ask the LLM for an input that will pass them.
//  TODO: start with an input that has gotten closest to the target line


*/

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
} | {
    type: 'custom',
    name: string,
});

export interface LeafParameter {
    mergedPath: string,
    path: ObjectPathSegment[],
    value: ValueGeneratedParameter['value'],
}

const specimenIdPrefixes = ['auto', 'custom'] as const
export type SpecimenId = `${typeof specimenIdPrefixes[number]}-${string}`

export function isSpecimenId(s:string):s is SpecimenId {
    const strimmed = s.trim();
    for (const prefix of specimenIdPrefixes) {
        if (s.startsWith(prefix) && strimmed.length > prefix.length) {
            return true;
        }
    }

    return false;
}

export type Specimen = BaseSpecimen & {
    id: SpecimenId,
    sequenceInType: number,
    sequence: number,
    leaves: LeafParameter[],
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
    depthLimit: number; //  INCLUDSIVE; <= depth limit is a-okay
    // weirdness: number;
    literals?: Literals;
}

interface GeneratorState {
    //  currentDepth and pathToHere are separate because for union types currentDepth doesn't increase
    //  but we want to include the union type in the path
    numberOfLevelsAvailable: number;
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

export const isEnumType = (type: ts.Type): type is ts.EnumType => {
    //  TODO: when will this be Enum and when EnumLiteral?
    return ((type.flags & ts.TypeFlags.Enum) !== 0
        || (type.flags & ts.TypeFlags.EnumLiteral) !== 0);
};

type Sizer = (o?: any) => Generator<number, any, any>;
type PropertyPicker = (k: string[], required: Set<string>) => Generator<string[], any, any>;
type ElementPicker = (max: number) => Generator<number, any, any>;

export type G = Generator<GeneratedParameter, any, any>;

export interface RuntimeContext {
    activeModule: any;
    weirdness: number;
    leafPeeping: Map<string, LeafParameter['value'][]>;
}

//  Critically important: the GeneratorWrapper is constructed upfront at analysis time,
//      before the function under test is tested and possibly in a different thread or process
//      its generator executes at runtime immediately before and possibly in a different thread or process
//  TODO: make this generic around its GeneratedParameter type?
interface GeneratorFactory {
    path: ObjectPathSegment[];
    shortest: number;
    longest: number;
    type: ts.Type,
    generator: (rc: RuntimeContext) => G,
}

type ValueGenerator = (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type, path: ObjectPathSegment[]) => GeneratorFactory | undefined;

//  cannot get weirder
const fixedValueGeneratorFactory = function* (generator: string, path: ObjectPathSegment[], value: any): G {
    const subtype = typeof value;
    if (!isValueSubtype(subtype)) {
        throw new Error(`Unexpected value type ${subtype}`);
    }

    const id = newId('value');
    while (true) {
        yield {
            id,
            generator,
            path,
            type: 'value',
            subtype,
            value,
        };
    }
};

//  cannot get weirder
const literalValueGeneratorFactory: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type, path: ObjectPathSegment[]) {
    if (type.isLiteral()) {
        const gw: GeneratorFactory = {
            path,
            type,
            shortest: 0,
            longest: 0,
            generator: (_: any) => fixedValueGeneratorFactory('literalValueGeneratorFactory', path, type.value),
        };
        return gw;
    }
    //  isLiteral() implementation inexplicably does not cover boolean literals
    //            return !!(this.flags & (128 /* StringLiteral */ | 256 /* NumberLiteral */ | 2048 /* BigIntLiteral */));
    if (type.flags & ts.TypeFlags.BooleanLiteral) {
        const t = checker.getTrueType();
        //  TODO: yuck
        const boolvalue = checker.typeToString(type) === checker.typeToString(t);

        const gw: GeneratorFactory = {
            path,
            type,
            shortest: 0,
            longest: 0,
            generator: (_: any) => fixedValueGeneratorFactory('literalValueGeneratorFactory', path, boolvalue),
        };
        return gw;
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

const simpleValueGeneratorFactory: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type, path: ObjectPathSegment[]) {
    if (simpleTypeFlags.includes(type.flags)) { //  TODO: is this a bitmask?
        const mergedPath = mergePath(path);
        //  I think this wrapping is necessary to keep Javascript from being confused about whether there's a generator here; returning immediately from a generator defined with function* without ever yielding still returns a Generator object
        const gSimpleValue = function* (rc: RuntimeContext) {
            while (true) {
                switch (type.flags) {
                    case ts.TypeFlags.Any:
                    case ts.TypeFlags.Unknown:
                        yield* edgyAny(path, configuration.literals);
                        break;
                    case ts.TypeFlags.String:
                        yield* edgyStrings(rc, path, configuration.literals);
                        break;
                    case ts.TypeFlags.Number:
                        yield* edgyNumbers(rc, path, configuration.literals);
                        break;
                    case ts.TypeFlags.Boolean:
                        yield* edgyBooleans(path, configuration.literals);
                        break;
                    default:
                        throw new Error(`Unexpected type ${type.flags}`);
                }
            }
        };
        const gw: GeneratorFactory = {
            path,
            type,
            shortest: 0,
            longest: 0,
            generator: gSimpleValue,
        };
        return gw;
    }
    return undefined;
};

//  cannot get weirder
const enumValueGeneratorFactory: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type, path: ObjectPathSegment[]): GeneratorFactory | undefined {
    if (isEnumType(type)) {
        if (type.symbol.valueDeclaration && ts.isEnumDeclaration(type.symbol.valueDeclaration)) {
            const actualEnumValues: (string | number)[] = [];
            for (const enumMember of type.symbol.valueDeclaration.members.values()) {
                const vv = checker.getConstantValue(enumMember);
                if (vv) {
                    actualEnumValues.push(vv);
                }
            }

            const gEnumValue = function* () {
                while (true) {
                    for (const v of actualEnumValues) {
                        const gp: GeneratedParameter = {
                            id: newId('enum'),
                            generator: 'enumValueGeneratorFactory',
                            type: 'value',
                            path,
                            subtype: 'enum',
                            value: 'noooooooooooooooooooo',
                        };
                        yield gp;
                    }
                }
            };
            const gw: GeneratorFactory = {
                path,
                type,
                shortest: 0,
                longest: 0,
                generator: gEnumValue,
            };
            return gw;
        }
        throw new Error(`Enum type ${checker.typeToString(type)} has no values`);
    }
};

/*

IF there are any subgenerators that can stay under the limit, pick from those

IF there are no subgenerators that can stay under the limit, get as close to the limit as possible and halt
OR throw an error

replace direct access to generators with a wrapper that knows shortest and longest
*/

const arrayValueGenerator: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type, path: ObjectPathSegment[]) {
    if (!checker.isArrayType(type)) {
        return;
    }

    const elementType = checker.getTypeArguments(type as ts.TypeReference)[0];
    const tts = checker.typeToString(elementType);

    function* generateEmpty() {
        while (true) {
            const gp: GeneratedParameter = {
                id: newId('empty-array'),
                generator: 'arrayValueGenerator',
                path,
                type: 'array',
                elements: [],
            };
            yield gp;
        }
    }

    if (state.numberOfLevelsAvailable <= 0) {
        const emptyGw: GeneratorFactory = {
            path,
            type,
            shortest: 0,
            longest: 0,
            generator: generateEmpty,
        };
        return emptyGw;
    }

    //  in some cases (okay maybe just this one number[]) we don't want to think of
    //  arrays as collections of unrelated elements
    //  we know state.numberOfLevelsAvailable > 0, and number[] has depth of 1, so we good
    if (elementType.flags & ts.TypeFlags.Number) {
        const numberRangyGw: GeneratorFactory = {
            path,
            type,
            shortest: 1,
            longest: 1,
            generator: (_: RuntimeContext) => edgyNumberRanges(checker, path, configuration.literals),
        };
        return numberRangyGw;
    }

    const newNumberOfLevelsAvailable = state.numberOfLevelsAvailable - 1;

    const newState: GeneratorState = {
        numberOfLevelsAvailable: newNumberOfLevelsAvailable,
    };

    const newPath = path.concat({
        typeString: checker.typeToString(elementType),
        segment: '[]',
        generator: 'arrayValueGenerator',
    });

    const elementGenerator = generatorator(configuration, checker, newState, elementType, newPath);

    if (!elementGenerator || elementGenerator.shortest > newNumberOfLevelsAvailable) {
        const emptyGw: GeneratorFactory = {
            path,
            type,
            shortest: 0,
            longest: 0,
            generator: generateEmpty,
        };
        return emptyGw;
    }

    const sizer = stupidSizer;
    const gArray = function* (rc: RuntimeContext): G {
        const egen = elementGenerator.generator(rc);
        while (true) {
            for (const count of sizer()) {
                const a = [];
                for (let i = 0; i < count; i++) {
                    const next = egen.next();
                    if (next.done) {
                        throw new Error(`Generator ${elementGenerator.constructor.name} is done`);
                    }
                    a.push(next.value);
                }

                yield {
                    id: newId('array'),
                    generator: 'arrayValueGenerator',
                    type: 'array',
                    path,
                    elements: a,
                };
            }
        }
    };

    const gw: GeneratorFactory = {
        path,
        type,
        shortest: 0,
        longest: 0,
        generator: gArray,
    };
    return gw;
};

//  TODO: IntersectionGenerator;
//  intersections are just objects
const intersectionValueGeneratorFactory: ValueGenerator = (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type, path: ObjectPathSegment[]) => {
    if (!type.isIntersection()) {
        return undefined;
    }

    const newNumberOfLevelsAvailable = state.numberOfLevelsAvailable - 1;

    const newState = {
        numberOfLevelsAvailable: newNumberOfLevelsAvailable,
    };

    const newPath = path.concat({
        typeString: checker.typeToString(type),
        segment: '&',
        generator: 'intersectionValueGeneratorFactory',
    });

    let shortest = 0;
    let longest = 0;
    const generappers: GeneratorFactory[] = [];
    for (const subtype of type.types) {
        const gw = generatorator(configuration, checker, newState, subtype, newPath);
        if (gw && gw.shortest <= newNumberOfLevelsAvailable) {
            generappers.push(gw);
            if (gw.shortest < shortest) {
                shortest = gw.shortest;
            }
            if (gw.longest > longest) {
                longest = gw.longest;
            }
        }
    }

    if (generappers.length === 0) {
        console.log(`No generators available within floor limit ${newNumberOfLevelsAvailable} <= ${configuration.depthLimit} for ${pathToString(checker, path)}`);
        return undefined;
    }

    function* gIntersection(rc: RuntimeContext): G {
        const generators: G[] = generappers.map(g => g.generator(rc));

        while (true) {
            //  intersecting types are always objects
            const combined: any = {};
            const required = new Set<string>();
            for (let i = 0; i < generappers.length; i++) {
                const next = generators[i].next();
                if (next.done) {
                    throw new Error(`Generator ${generappers[i].constructor.name} is done`);
                }
                //  TODO: enforce at compile time that this is an object; maybe some generics are called for?
                const o = next.value;
                if (o.type !== 'object') {
                    throw new Error(`Unexpected type ${o.type} in intersection`);
                }
                Object.assign(combined, o.properties);
                for (const k of o.required) {
                    required.add(k);
                }
            }

            const gp: GeneratedParameter = {
                id: newId('intersection'),
                generator: 'intersectionValueGeneratorFactory',
                type: 'object',
                properties: combined,
                required: Array.from(required),
                path,
                declaredType: checker.typeToString(type),
            };

            yield gp;
        }
    }

    //  count the current intersection as a step
    shortest++;
    longest++;
    const gw: GeneratorFactory = {
        path,
        type,
        shortest,
        longest,
        generator: gIntersection,
    };

    return gw;
};

const unionValueGeneratorFactory: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type, path: ObjectPathSegment[]) {
    if (!type.isUnion()) {
        return undefined;
    }

    const bastttes = checker.typeToString(type);
    const newNumberOfLevelsAvailable = state.numberOfLevelsAvailable;   //  do NOT subtract 1

    const newState = state;

    let shortest = 0;
    let longest = 0;
    const generappers: GeneratorFactory[] = [];
    for (let i = 0; i < type.types.length; i++) {
        const tts = checker.typeToString(type.types[i]);
        const gw = generatorator(configuration, checker, newState, type.types[i], path);
        if (gw && gw.shortest <= newNumberOfLevelsAvailable) {
            generappers.push(gw);
            if (gw.shortest < shortest) {
                shortest = gw.shortest;
            }
            if (gw.longest > longest) {
                longest = gw.longest;
            }
        }
    }

    if (generappers.length === 0) {
        console.log(`No generators available at depth ${newNumberOfLevelsAvailable} <= ${configuration.depthLimit}; ${pathToString(checker, path)}`);
        return undefined;
    }

    const gUnion = function* (rc: RuntimeContext) {
        const generators = generappers.map(g => g.generator(rc));

        while (true) {
            //  TODO: run the shorter depth ones first
            for (let i = 0; i < generappers.length; i++) {
                const next = generators[i].next();
                if (next.done) {
                    throw new Error(`Generator ${generappers[i].constructor.name} is done`);
                }
                const gp = next.value;
                yield gp;
            }
        }
    };

    const gw: GeneratorFactory = {
        path,
        type,
        shortest,
        longest,
        generator: gUnion,
    };
    return gw;
};

function pathToString(checker: ts.TypeChecker, path: ObjectPathSegment[]) {
    return path.map(p => `${p.segment}`).join('');
}

//  does NOT validate its argument
const mapValueGeneratorFactory: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type, path: ObjectPathSegment[]) {
    if (!isTypeReference(type)) {
        throw new Error(`Unexpected type not a reference ${checker.typeToString(type)}`);
    }

    const sizer = stupidSizer;

    function* generateEmpty() {
        const empty: GeneratedParameter = {
            id: newId('empty-map'),
            generator: 'mapValueGenerator',
            path,
            type: 'map',
            entries: [],
        };

        while (true) {
            yield empty;
        }
    }

    const [keyType, valueType] = (() => {
        if (type.typeArguments && type.typeArguments.length === 2) {
            return type.typeArguments;
        }
        //  when types are not specified, just go string=>string
        return [checker.getStringType(), checker.getStringType()];
    })();

    const newNumberOfLevelsAvailable = state.numberOfLevelsAvailable - 1;
    const newState: GeneratorState = {
        numberOfLevelsAvailable: newNumberOfLevelsAvailable,
    };

    const newKeyPath = path.concat({
        typeString: checker.typeToString(type),
        segment: '.key',
        generator: 'mapValueGenerator',
    });
    const keyGeneratorFactory = generatorator(configuration, checker, newState, keyType, newKeyPath);

    const newValuePath = path.concat({
        typeString: checker.typeToString(type),
        segment: '.value',
        generator: 'mapValueGenerator',
    });
    const valueGeneratorFactory = generatorator(configuration, checker, newState, valueType, newValuePath);

    if (!keyGeneratorFactory || keyGeneratorFactory.shortest > newNumberOfLevelsAvailable || !valueGeneratorFactory || valueGeneratorFactory.shortest > newNumberOfLevelsAvailable) {
        const gw: GeneratorFactory = {
            path,
            type,
            shortest: 0,
            longest: 0,
            generator: generateEmpty,
        };
        return gw;
    }

    const gMap = function* (rc: RuntimeContext): G {
        //  the newNumberOfLevelsAvailable <= 0 check seems like a band aid for an undiagnosed
        //  level counting bug somewhere else

        const keyGenerator = keyGeneratorFactory.generator(rc);
        const valueGenerator = valueGeneratorFactory.generator(rc);
        while (true) {
            for (const count of sizer()) {
                const entries: [GeneratedParameter, GeneratedParameter][] = [];
                for (let i = 0; i < count; i++) {
                    const key = keyGenerator.next();
                    if (key.done) {
                        throw new Error(`Key generator is done ${pathToString(checker, keyGeneratorFactory.path)}`);
                    }
                    const value = valueGenerator.next();
                    if (value.done) {
                        throw new Error(`Value generator is done ${pathToString(checker, valueGeneratorFactory.path)}`);
                    }
                    entries.push([key.value, value.value]);
                }
                yield {
                    id: newId('map'),
                    generator: 'mapValueGenerator',
                    type: 'map',
                    path,
                    entries,
                };
            }
        }
    };

    const shortestChild = Math.min(keyGeneratorFactory.shortest, valueGeneratorFactory.shortest);
    const longestChild = Math.max(keyGeneratorFactory.longest, valueGeneratorFactory.longest);

    const gw: GeneratorFactory = {
        path,
        type,
        shortest: shortestChild + 1,
        longest: longestChild + 1,
        generator: gMap,
    };

    return gw;
};

const setValueGeneratorFactory: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type, path: ObjectPathSegment[]) {
    if (!isTypeReference(type)) {
        throw new Error(`Unexpected type not a reference ${checker.typeToString(type)}`);
    }

    const sizer = stupidSizer;
    const elementType = type.typeArguments?.length === 1 ? type.typeArguments[0] : checker.getStringType();

    function* generateEmpty() {
        const empty: GeneratedParameter = {
            id: newId('empty-set'),
            generator: 'setValueGenerator',
            type: 'set',
            path,
            entries: [],
        };

        while (true) {
            yield empty;
        }
    }

    const newPath = path.concat({
        typeString: checker.typeToString(type),
        segment: '.element',
        generator: 'setValueGenerator',
    });

    const newNumberOfLevelsAvailable = state.numberOfLevelsAvailable - 1;

    const newState = {
        numberOfLevelsAvailable: newNumberOfLevelsAvailable,
    };

    const elementGeneratorFactory = generatorator(configuration, checker, newState, elementType, newPath);
    if (!elementGeneratorFactory?.generator || elementGeneratorFactory.shortest > newNumberOfLevelsAvailable) {
        const gw: GeneratorFactory = {
            path,
            type,
            shortest: 0,
            longest: 0,
            generator: generateEmpty,
        };
        return gw;
    }

    const gSet = function* (rc: RuntimeContext): G {
        //  when unspecified make it a string
        const elementGenerator = elementGeneratorFactory.generator(rc);
        while (true) {
            for (const count of sizer()) {
                const entries: GeneratedParameter[] = [];
                for (let i = 0; i < count; i++) {
                    const next = elementGenerator.next();
                    if (next.done) {
                        throw new Error(`Generator ${elementGeneratorFactory.constructor.name} is done`);
                    }
                    entries.push(next.value);
                }
                yield {
                    id: newId('set'),
                    generator: 'setValueGeneratorFactory',
                    type: 'set',
                    path,
                    entries,
                };
            }
        }
    };

    const gw: GeneratorFactory = {
        path,
        type,
        shortest: 1 + elementGeneratorFactory.shortest,
        longest: 1 + elementGeneratorFactory.longest,
        generator: gSet,
    };

    return gw;
};

const dateValueGeneratorFactory: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type, path: ObjectPathSegment[]) {

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

    function* gDate(rc: RuntimeContext): G {
        rc.weirdness;
        for (const neighborOffset of neighbors) {
            for (const perturbationMultiplier of [0, 1, -1, 2, -2]) {
                for (const perturbation of perturbations) {
                    for (const baseDateEpoch of baseDatesEpoch) {
                        const epochMs = baseDateEpoch + perturbation * perturbationMultiplier + neighborOffset;
                        yield {
                            id: newId('date'),
                            generator: 'dateValueGeneratorFactory',
                            type: 'date',
                            path,
                            epochMs,
                        };
                    }
                }
            }
        }
    }

    const gw: GeneratorFactory = {
        path,
        type,
        shortest: 0,
        longest: 0,
        generator: gDate,
    };
    return gw;
};

const regexpValueGeneratorFactory: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type, path: ObjectPathSegment[]) {
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

    function* gRegexp(rc: RuntimeContext): G {
        rc.weirdness;
        while (true) {
            for (const pattern of patterns) {
                yield {
                    id: newId('pattern'),
                    generator: 'regexpValueGeneratorFactory',
                    type: 'regexp',
                    path,
                    pattern: pattern.toString(),
                };
            }
        }
    }

    const gw: GeneratorFactory = {
        path,
        type,
        shortest: 0,
        longest: 0,
        generator: gRegexp,
    };
    return gw;
};

const basicObjectValueGeneratorFactory: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type, path: ObjectPathSegment[]) {
    //  in theory this can be a parameter in the future
    const picker = stupidPropertyPicker;

    const declaredType = checker.typeToString(type);

    const newNumberOfLevelsAvailable = state.numberOfLevelsAvailable - 1;

    const newState = {
        numberOfLevelsAvailable: newNumberOfLevelsAvailable,
    };

    const propertyGeneratorFactories: Record<string, GeneratorFactory> = {};
    const required = new Set<string>();

    let shortestChild = 0;
    let longestChild = 0;
    for (const p of checker.getPropertiesOfType(type)) {
        const declaration = getDeclaration(p);
        if (declaration) {
            const propertyType = checker.getTypeOfSymbolAtLocation(p, declaration);
            const pgw = generatorator(configuration, checker, newState, propertyType, path.concat({
                typeString: checker.typeToString(type),
                segment: `["${p.name}"]`,
                generator: 'basicObjectValueGeneratorFactory',
            }));

            const tts = checker.typeToString(propertyType);

            /*
                                REQUIRED                                    NOT REQUIRED
                                PROVIDED            NOT PROVIDED            PROVIDED            NOT PROVIDED
                        
            ALLOWED             YES                 ERROR                   YES                 NO         
            NOT ALLOWED         ERROR               ERROR                   NO                  NO

            */

            const isRequired = !(p.flags & ts.SymbolFlags.Optional);
            if (isRequired) {
                if (!pgw) {
                    console.log(`Required property ${p.name}:${checker.typeToString(propertyType)} cannot be generated at depth ${configuration.depthLimit}: ${pathToString(checker, path)}`);
                    return undefined;
                }
                required.add(p.name);
            } else {
                if (!pgw) {
                    continue;
                }
            }

            const isAllowed = pgw.shortest <= newNumberOfLevelsAvailable;
            if (!isAllowed && required.has(p.name)) {
                //  TODO: custom error type
                console.log(`Required property ${p.name}:${checker.typeToString(propertyType)} cannot be generated for required depth ${pgw.shortest} <= ${configuration.depthLimit}: ${pathToString(checker, path)}`);
                return undefined;
            }

            if (isAllowed) {
                propertyGeneratorFactories[p.name] = pgw;
                if (pgw.shortest < shortestChild) {
                    shortestChild = pgw.shortest;
                }
                if (pgw.longest > longestChild) {
                    longestChild = pgw.longest;
                }
            }
        }
    }

    const gBasicObject = function* (rc: RuntimeContext): G {
        //  TODO: skip optional properties first, then add optional properties in order from shortest to longest
        const keysGenerator = picker(Array.from(Object.keys(propertyGeneratorFactories)), required);

        const propertyGenerators = Object.fromEntries(
            Object.entries(propertyGeneratorFactories).map(([k, v]) =>
                [k, v.generator(rc)]
            ));

        while (true) {
            for (const keys of keysGenerator) {
                const o: Record<string, GeneratedParameter> = {};
                for (const k of keys) {
                    const key = k as string;

                    const next = propertyGenerators[key].next();
                    if (next.done) {
                        throw new Error(`Generator ${key} is done`);
                    }

                    const v = rehydrateGeneratedParameterValue(next.value, rc);
                    if (v === undefined && required.has(key)) {
                        console.log(`Required property ${key} is undefined at depth ${newNumberOfLevelsAvailable} ${pathToString(checker, path)}}`);
                    }

                    o[key] = next.value;
                }

                yield {
                    id: newId('object'),
                    generator: 'basicObjectValueGeneratorFactory',
                    type: 'object',
                    path,
                    properties: o,
                    required: Array.from(required),
                    declaredType,
                };
            }
        }
    };

    //  count the current object as a step
    const gw: GeneratorFactory = {
        path,
        type,
        shortest: shortestChild + 1,
        longest: longestChild + 1,
        generator: gBasicObject,
    };
    return gw;
};

const functionValueGeneratorFactory: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type, path: ObjectPathSegment[]) {
    const callSignatures = checker.getSignaturesOfType(type, ts.SignatureKind.Call);

    if (callSignatures.length !== 1) {
        throw new Error(`Don't know what to do with ${callSignatures.length} call signatures`);
    }

    const returnType = callSignatures[0].getReturnType();

    const newState: GeneratorState = {
        numberOfLevelsAvailable: configuration.depthLimit,    //  function generators restart the depth counting because they're different object trees
    };

    const g = generatorator(configuration, checker, newState, returnType, path.concat({
        typeString: checker.typeToString(type),
        segment: '()=>',
        generator: 'functionValueGeneratorFactory',
    }));

    if (!g) {
        //  ????
        throw new Error(`Function return type ${checker.typeToString(returnType)} cannot be generated at depth ${configuration.depthLimit}: ${pathToString(checker, path)}`);
    }

    if (g.shortest > configuration.depthLimit) {
        //  ????
        throw new Error(`Function return type ${checker.typeToString(returnType)} cannot be generated at depth ${g.shortest} <= ${configuration.depthLimit}: ${pathToString(checker, path)}`);
    }

    const gFunctionValue = function* (rc: RuntimeContext): G {
        const rvGenerator = g.generator(rc);
        while (true) {
            for (const returnValue of rvGenerator) {
                yield {
                    id: newId('function'),
                    generator: 'functionValueGeneratorFactory',
                    type: 'callable',
                    path,
                    returnValue,
                };
            }
        }
    };

    const gw: GeneratorFactory = {
        path,
        type,
        shortest: 0,
        longest: 0,
        generator: gFunctionValue,
    };
    return gw;
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

const objectValueGeneratorFactory: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type, path: ObjectPathSegment[]) {
    if (!(type.flags & ts.TypeFlags.Object)) {
        return;
    };

    //  TODO: find a better way to detect if the type is a function type
    const callSignatures = checker.getSignaturesOfType(type, ts.SignatureKind.Call);
    if (callSignatures.length > 0) {
        return functionValueGeneratorFactory(configuration, checker, state, type, path);
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
            return mapValueGeneratorFactory(configuration, checker, state, type, path);
        }

        if (typeName === 'Set') {
            return setValueGeneratorFactory(configuration, checker, state, type, path);
        }

        if (typeName === 'Date') {
            return dateValueGeneratorFactory(configuration, checker, state, type, path);
        }

        if (typeName === 'RegExp') {
            return regexpValueGeneratorFactory(configuration, checker, state, type, path);
        }

        //  TODO: Symbol
        //  https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects

        //  TODO: Promise

        throw new Error(`Not ready for default global type ${typeName}`);
    }

    if (type.isClass()) {   //  only detects user-defined classes

    }

    return basicObjectValueGeneratorFactory(configuration, checker, state, type, path);
};

function construct(type: ts.InterfaceType, constructor: ts.Signature, args: any[]) {
}



const classValueGeneratorFactory: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type, path: ObjectPathSegment[]) {
    if (!type.isClass()) {
        return;
    }

    //  for mysterious typescript reasons, we have to dig one level deeper
    //  to get the reference to the class that can provide the actual constructors
    //  maybe there's some type reference indirection?
    const stype = checker.getTypeOfSymbol(type.symbol);
    const constructors = checker.getSignaturesOfType(stype, ts.SignatureKind.Construct);
    // const constructors = checker.getSignaturesOfType(type, ts.SignatureKind.Construct);

    // console.log(`type = ${checker.typeToString(type)}; symbol type = ${checker.typeToString(stype)} with ${constructors.length}`);
    // console.log(`type = ${checker.typeToString(type)} with ${constructors.length}; symbol type = ${checker.typeToString(stype)} with ${sconstructors.length}`);

    // const constructors = type.getConstructSignatures();
    if (constructors.length === 0) {
        throw new Error(`Class ${checker.typeToString(type)} has no constructors`);
    }

    if (constructors.length > 1) {
        console.log(`Class ${checker.typeToString(type)} has multiple constructors; I guess now we know how that happens`);
    }

    //  constructor parameters count against our complexity limits because they're inputs
    //  and thus are rooted in the same object tree as the initial call much like
    //  object properties or array elements etc.
    //  in contrast function types generate the return value, which is a separate object tree
    const newNumberOfLevelsAvailable = state.numberOfLevelsAvailable - 1;

    const newPath = path.concat({
        typeString: checker.typeToString(type),
        segment: '.new()=>',
        generator: 'classValueGeneratorFactory',
    });

    let overallShortest = 0;
    let overallLongest = 0;
    const constructorGenerators: GeneratorFactory[] = [];
    for (let i = 0; i < constructors.length; i++) {
        const parameters = constructors[i].getParameters();

        const parametersExceedingMinimumDepth: number[] = [];
        const generappersByPosition: GeneratorFactory[] = [];
        let generappersByType = new Map<ts.Type, GeneratorFactory>();
        //  don't try to convert this to the factory/generator style because function declarations require
        //  an AST Node not just a type
        // const generators: G[] = [];

        let shortest = 0;
        let longest = 0;
        for (let j = 0; j < parameters.length; j++) {
            const currentType = checker.getTypeOfSymbolAtLocation(parameters[j], constructors[i].getDeclaration());
            let generapper = generappersByType.get(currentType);
            if (!generapper) {
                generapper = generatorator(configuration, checker, state, currentType, []);
                if (generapper) {
                    const t = checker.typeToString(currentType);
                    generappersByType.set(currentType, generapper);
                    if (generapper.shortest > newNumberOfLevelsAvailable) {
                        parametersExceedingMinimumDepth.push(j);
                    }
                    generappersByPosition.push(generapper);
                    if (shortest > generapper.shortest) {
                        shortest = generapper.shortest;
                    }
                    if (longest < generapper.longest) {
                        longest = generapper.longest;
                    }
                } else {
                    parametersExceedingMinimumDepth.push(j);
                }
            }
        }

        if (parametersExceedingMinimumDepth.length === 0) {
            const fqn = checker.getFullyQualifiedName(type.symbol);

            //  RUN TIME
            function* gConstructor(rc: RuntimeContext) {
                const generators = generappersByPosition.map(g => g.generator(rc));
                while (true) {
                    const args: GeneratedParameter[] = [];
                    for (let i = 0; i < generappersByPosition.length; i++) {
                        const next = generators[i].next();
                        if (next.done) {
                            throw new Error(`Generator ${checker.typeToString(generappersByPosition[i].type)} is done`);
                        }

                        args.push(next.value);
                    }

                    const gp: GeneratedParameter = {
                        id: newId('class'),
                        generator: 'classValueGeneratorFactory',
                        type: 'class',
                        path,
                        fullyQualifiedName: fqn,
                        parameters: args,
                    };
                    yield gp;
                }
            };

            if (shortest < overallShortest) {
                overallShortest = shortest;
            }

            if (longest > overallLongest) {
                overallLongest = longest;
            }

            const constructorGw: GeneratorFactory = {
                path: newPath,
                shortest,
                longest,
                type,
                generator: gConstructor,
            };
            constructorGenerators.push(constructorGw);
        }
    }

    if (constructorGenerators.length === 0) {
        return;
    }

    //  TODO: make it clearer that the generators are at runtime versus the rest of this, which is at load time
    const gClass = function* (rc: RuntimeContext): G {
        const generators = constructorGenerators.map(g => g.generator(rc));
        while (true) {
            for (let i = 0; i < constructorGenerators.length; i++) {
                const next = generators[i].next();
                if (next.done) {
                    throw new Error(`Generator ${checker.typeToString(constructorGenerators[i].type)} is done`);
                }

                yield next.value;
            }
        }
    };

    const classGW: GeneratorFactory = {
        path: newPath,
        shortest: 1 + overallShortest,
        longest: 1 + overallLongest,
        type,
        generator: gClass,
    };

    return classGW;
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

//  IFF unseen type and no children -- shortest = 0, longest = 0
//  if seen type and no children -- shortest = Infinity, longest = Infinity
//  shortest = 0 for collection types - array, map, set, and objects with only optional properties; empty collections are treated like scalars
//  shortest = 1 for callables
//  shortest = 0 + min(children) for unions
//  shortest = 1 + min(children) for non-unions
//  longest = 1 for callables
//  longest = 0 + max(children) for unions
//  longest = 1 + max(children) for non-unions, including collections and objects

/*
object flags        Reference = 4,
object flags        Anonymous = 16,

symbol flags         TypeAlias = 524288,
type flags         Object = 524288,


object flags 524288 = ??? maybe object again?
object flags 524368 = ??? Instantiated & Anonymous
*/

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

const getDeclaration = (symbol: ts.Symbol) => {
    const declarations = symbol.getDeclarations();
    if (declarations && declarations.length > 0) {
        return declarations[0];
    }
    if (symbol.valueDeclaration) {
        console.log(`symbol ${symbol.getName()} has value declaration ${symbol.valueDeclaration} but no other declarations`);
        return symbol.valueDeclaration;
    }

    throw new Error(`No declaration for symbol ${symbol.getName()}`);
};

//  returns null if there is insufficient depth to generate a value
function generatorator(configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type, path: ObjectPathSegment[]): GeneratorFactory | undefined {
    if (state.numberOfLevelsAvailable < 0) {
        return undefined;
    }

    // TODO: iterables and generators, regular expressions, promises, tagged templates, and more
    const factories: ValueGenerator[] = [
        literalValueGeneratorFactory,
        simpleValueGeneratorFactory,
        enumValueGeneratorFactory,
        arrayValueGenerator,
        intersectionValueGeneratorFactory,
        unionValueGeneratorFactory,
        classValueGeneratorFactory,
        objectValueGeneratorFactory,
    ];

    for (const factory of factories) {
        const generator = factory(configuration, checker, state, type, path);
        if (generator) {
            return generator;
        }
    }

    console.log(`Unable to handle type ${checker.typeToString(type)} within depth ${state.numberOfLevelsAvailable}`);
}

//  construct a stateful hierarchy of generators    
function* functionGeneratorator(checker: ts.TypeChecker, f: ts.FunctionDeclaration, runtimeContext: RuntimeContext, literals?: Literals): Generator<GeneratedParameter[], any, any> {
    if (f.parameters.length === 0) {
        yield [];
        return;
    }

    const MAXIMUM_DEPTH = 6;

    //  save the generators by depth to resume at previous point in stream instead of restarting
    //  TODO: this can probably itself be put into generator state
    let generappersByTypeByDepth = new Map<number, Map<ts.Type, GeneratorFactory>>();
    let generatorsByTypeByDepth = new Map<number, Map<ts.Type, G>>();
    //  do a batch at each depth level then start again from the lowest
    while (true) {
        for (let currentDepthLimit = 0; currentDepthLimit < MAXIMUM_DEPTH; currentDepthLimit++) {
            //  1p, 4p, 9p, 16p, 25p
            const maxValuesAtThisDepth = f.parameters.length * 2 * ((1 + currentDepthLimit) ** 2);
            let valuesYieldedAtThisDepth = 0;

            const state: GeneratorState = {
                numberOfLevelsAvailable: currentDepthLimit,
            };

            const configuration: GeneratorConfiguration = {
                depthLimit: currentDepthLimit,
                // weirdness: runtimeContext.weirdness,
                literals,
            };

            const parametersExceedingMinimumDepth: number[] = [];
            let generappersByType = generappersByTypeByDepth.get(currentDepthLimit);
            let generatorsByType = generatorsByTypeByDepth.get(currentDepthLimit);
            const generappersByPosition: GeneratorFactory[] = [];
            const generatorsByPosition: G[] = [];
            if (!generappersByType || !generatorsByType) {
                generappersByType = new Map<ts.Type, GeneratorFactory>();
                generatorsByType = new Map<ts.Type, G>();
                //  don't try to convert this to the factory/generator style because function declarations require
                //  an AST Node not just a type
                // const generators: G[] = [];

                for (let j = 0; j < f.parameters.length; j++) {
                    const t = f.parameters[j].type;
                    const currentType = t
                        ? checker.getTypeAtLocation(t)
                        : checker.getAnyType();

                    let generapper = generappersByType.get(currentType);
                    if (!generapper) {
                        generapper = generatorator(configuration, checker, state, currentType, []);
                        // console.log(`generapper for ${checker.typeToString(currentType)} is ${generapper?.shortest} to ${generapper?.longest}`);
                        if (generapper) {
                            const t = checker.typeToString(currentType);
                            generappersByType.set(currentType, generapper);
                            if (generapper.shortest > currentDepthLimit) {
                                parametersExceedingMinimumDepth.push(j);
                            }
                            generappersByPosition.push(generapper);

                            const generator = generapper.generator(runtimeContext);
                            generatorsByType.set(currentType, generator);
                            generatorsByPosition.push(generator);
                        } else {
                            parametersExceedingMinimumDepth.push(j);
                        }
                    }
                }
            }

            if (parametersExceedingMinimumDepth.length > 0) {
                console.log(`${parametersExceedingMinimumDepth.length} parameters exceed minimum depth ${currentDepthLimit}; skipping - ${parametersExceedingMinimumDepth.map(i => `${i}:${generappersByPosition[i]?.shortest ?? -1}`).join(', ')}}`);
                continue;
            }

            while (valuesYieldedAtThisDepth < maxValuesAtThisDepth) {
                //  generate exactly one (1) value for each parameter type; this guarantees that we sometimes pass in identical values
                //  variations

                const numberOfValuesPerType = Math.max(4, f.parameters.length);
                const valuesByType = new Map<ts.Type, GeneratedParameter[]>();
                for (const [type, generator] of generatorsByType.entries()) {
                    //  get multiple values for each parameter type for blending below
                    const parameterValues: GeneratedParameter[] = [];
                    for (let j = 0; j < numberOfValuesPerType; j++) {
                        const next = generator.next();
                        if (next.done) {
                            throw new Error(`Generator for ${checker.typeToString(type)} is done`);
                        }
                        parameterValues.push(next.value);
                    }
                    valuesByType.set(type, parameterValues);
                };

                const arbitrarySkipProbability = 0.1;
                for (let k = 0; k < f.parameters.length; k++) {
                    const newValues: any[] = [];
                    for (let j = 0; j < f.parameters.length; j++) {
                        //  if this parameter is optional, sometimes skip it and the rest
                        if (f.parameters[j].questionToken) {
                            if (Math.random() < arbitrarySkipProbability) {
                                break;
                            }
                        }

                        const t = f.parameters[j].type;
                        const currentType = t
                            ? checker.getTypeAtLocation(t)
                            : checker.getAnyType();

                        /*
                            we should get parameter lists (the number for % p is the index into the per-type value list)
                            k = 0
                            j =         1   2   3   4
                            j + k =     1   2   3   4
                            % p =       1   2   3   0

                            k = 1
                            j =         1   2   3   4
                            j + k =     2   3   4   5
                            %p =        2   3   0   1

                            k = 2
                            j =         1   2   3   4
                            j + k =     3   4   5   6
                            %p =        3   0   1   2
                            
                            k = 3
                            j =         1   2   3   4
                            j + k =     4   5   6   7
                            %p =        0   1   2   3

                            this does not get a complete Cartesian product of all values, but that's okay; we have hybridization happening later
                        */
                        const valueIndex = (k + j) % numberOfValuesPerType;
                        const v = valuesByType.get(currentType)?.[valueIndex];
                        newValues.push(v);
                    }

                    yield newValues;
                }
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

    *seeder(runtimeContext: RuntimeContext, literals?: Literals): Generator<BaseSpecimen, any, any> {
        const f = this.f;
        const checker = this.checker;

        //  TODO: using TupleGenerator and then unpacking like this... needlessly elaborate?
        const generator = functionGeneratorator(checker, f, runtimeContext, literals);
        for (const value of generator) {
            const s: BaseSpecimen = {
                parameters: value,
                type: 'seed',
            };
            yield s;
        }
    }

    increaseWeirdness(increment = 1): void {
        this.weirdness += increment;
    }
}