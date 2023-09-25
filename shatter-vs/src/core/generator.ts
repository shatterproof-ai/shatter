import * as ts from 'typescript';

import { createId } from "@paralleldrive/cuid2";

import { ResultCluster } from '../core/shatter';
import { RunResult } from '../core/supervisor';
import { GeneratedParameter, edgyAny, edgyBooleans, edgyNumbers, edgyStrings } from './seed';
import { set } from 'lodash';

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

interface GeneratorConfiguration {
    maxDepth: number;
    weirdness: number;
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
        && type.objectFlags === ts.ObjectFlags.Reference;
};

const isEnumType = (type: ts.Type): type is ts.EnumType => {
    //  TODO: when will this be Enum and when EnumLiteral?
    return type.flags === ts.TypeFlags.Enum || type.flags === ts.TypeFlags.EnumLiteral;
};

type Sizer = (o?: any) => Generator<number, any, any>;
type PropertyPicker = (k: string[]) => Generator<string[], any, any>;
type ElementPicker = (max: number) => Generator<number, any, any>;

type G = Generator<GeneratedParameter, any, any>;
type ValueGenerator = (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type) => G | undefined;

const fixedValueGeneratorFactory = function* (generator: string, value: any): G {
    const id = createId();
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
    if (simpleTypeFlags.includes(type.flags)) {
        //  I think this wrapping is necessary to keep Javascript from being confused about whether there's a generator here; returning immediately from a generator defined with function* without ever yielding still returns a Generator object
        const g = function* () {
            while (true) {
                switch (type.flags) {
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
                        throw new Error(`Unexpected type ${type.flags}`);
                }
            }
        };
        return g();
    }
    return undefined;
};

const enumValueGeneratorFactory: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type):G|undefined {
    if (isEnumType(type)) {
        const enumValues = type.symbol.members;
        if (enumValues) {
            const g = function* () {
                while (true) {
                    for (const v of enumValues) {
                        const gp: GeneratedParameter = {
                            id: createId(),
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
    generateEmpty: () => GeneratedParameter;
    generate: (configuration: GeneratorConfiguration, state: GeneratorState) => G;
};

const stateAwareGenerator = function* (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type, g: TwoPhaseGenerator) {
    const currentDepth = state.currentDepth;
    while (true) {
        if (state.currentDepth >= configuration.maxDepth) {
            const gg = g.generateEmpty();
            while (currentDepth >= configuration.maxDepth) {
                yield gg;
            }
        } else {
            const gg = g.generate(configuration, state);
            const operatingWeirdness = configuration.weirdness;
            while (currentDepth <= configuration.maxDepth || operatingWeirdness !== configuration.weirdness) {
                const v = gg.next();
                if (v.done) {
                    throw new Error(`Generator ${gg.constructor.name} is done`);
                }
                yield v.value;
            }
        }
    }
};

const arrayValueGenerator: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type): G | undefined {
    if (checker.isArrayType(type)) {
        return;
    }

    const generateEmpty = (): GeneratedParameter => ({
        id: createId(),
        generator: 'arrayValueGenerator',
        type: 'array',
        range: [],
    });

    const generate = function* (configuration: GeneratorConfiguration, state: GeneratorState): G {
        const elementType = checker.getTypeArguments(type as ts.TypeReference)[0];
        const newState: GeneratorState = {
            currentDepth: state.currentDepth + 1,
            pathToHere: state.pathToHere.concat(".[]"),
        };

        const sizer = stupidSizer;

        const elementGenerator = generatorator(configuration, checker, newState, elementType);
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
                    id: createId(),
                    generator: 'arrayValueGenerator',
                    type: 'array',
                    range: a,
                };
            }
        }
    };

    return stateAwareGenerator(configuration, checker, state, type, {
        generateEmpty,
        generate,
    });
};

//  TODO: IntersectionGenerator;
//  intersections are just objects
const intersectionValueGeneratorFactory: ValueGenerator = function* (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type) {
    if (!type.isIntersection()) {
        return undefined;
    }
    const intersectingTypes = type.types;
    //  presumably must be an object type
    throw new Error("Not ready for intersectionality");

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
};

const unionValueGeneratorFactory: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type):G|undefined {
    if (type.isUnion()) {
        const g = function* () {
            const unionTypes = type.types;
            const generators: Generator<GeneratedParameter, any, any>[] = [];
            for (const unionType of unionTypes) {
                const newState = {
                    currentDepth: state.currentDepth,
                    pathToHere: state.pathToHere.concat(" | "),
                };
                const g = generatorator(configuration, checker, newState, unionType);
                generators.push(g);
            }

            const picker = stupidPicker;
            while (true) {
                for (const index of picker(generators.length)) {
                    const generator = generators[index];
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
    }
};

//  does NOT validate its argument
const mapValueGeneratorFactory: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type) {
    if (!isTypeReference(type)) {
        throw new Error(`Unexpected type not a reference ${checker.typeToString(type)}`);
    }

    const sizer = stupidSizer;

    const generateEmpty = (): GeneratedParameter => ({
        id: createId(),
        generator: 'mapValueGenerator',
        type: 'class',
        instance: new Map(),
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
        }, keyType);

        const valueGenerator = generatorator(configuration, checker, {
            currentDepth: updepth,
            pathToHere: state.pathToHere.concat('.value'),
        }, valueType);

        while (true) {
            for (const count of sizer()) {
                const m = new Map();
                for (let i = 0; i < count; i++) {
                    const key = keyGenerator.next();
                    if (key.done) {
                        throw new Error(`Generator ${keyGenerator.constructor.name} is done`);
                    }
                    const value = valueGenerator.next();
                    if (value.done) {
                        throw new Error(`Generator ${valueGenerator.constructor.name} is done`);
                    }
                    m.set(key.value, value.value);
                }
                yield {
                    id: createId(),
                    generator: 'mapValueGenerator',
                    type: 'class',
                    instance: m,
                };
            }
        }
    };

    return stateAwareGenerator(configuration, checker, state, type, {
        generateEmpty,
        generate,
    });
};

const setValueGeneratorFactory = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type) {
    if (!isTypeReference(type)) {
        throw new Error(`Unexpected type not a reference ${checker.typeToString(type)}`);
    }

    const sizer = stupidSizer;

    const generateEmpty = (): GeneratedParameter => ({
        id: createId(),
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
        const elementGenerator = generatorator(configuration, checker, newState, elementType);
        while (true) {

            for (const count of sizer()) {
                const s = new Set();
                for (let i = 0; i < count; i++) {
                    const next = elementGenerator.next();
                    if (next.done) {
                        throw new Error(`Generator ${elementGenerator.constructor.name} is done`);
                    }
                    s.add(next.value);
                }
                yield {
                    id: createId(),
                    generator: 'setValueGeneratorFactory',
                    type: 'class',
                    instance: s,
                };
            }
        }
    };

    return stateAwareGenerator(configuration, checker, state, type, {
        generateEmpty,
        generate,
    });
};

const basicObjectValueGeneratorFactory: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type) {
    //  in theory this can be a parameter in the future
    const picker = stupidPropertyPicker;

    const generateEmpty = (): GeneratedParameter => ({
        id: createId(),
        generator: 'basicObjectValueGenerator',
        type: 'object',
        properties: {},
    });

    const generate = function* (configuration: GeneratorConfiguration, state: GeneratorState): G {
        const propertyGenerators: Record<string, G> = {};
        checker.getPropertiesOfType(type).forEach(p => {
            if (p.valueDeclaration) {
                const propertyType = checker.getTypeOfSymbolAtLocation(p, p.valueDeclaration);

                const newState = {
                    currentDepth: state.currentDepth + 1,
                    pathToHere: state.pathToHere.concat(`.${p.name}`),
                };

                propertyGenerators[p.name] = generatorator(configuration, checker, newState, propertyType);
            }
        });

        const allProperties = Object.keys(propertyGenerators);
        const keysGenerator = picker(allProperties);

        while (true) {
            for (const keys of keysGenerator) {
                const o: Record<string, GeneratedParameter> = {};
                for (const k of keys) {
                    const key = k as string;
                    const next = propertyGenerators[key].next();
                    if (next.done) {
                        throw new Error(`Generator ${key} is done`);
                    }

                    o[key] = next.value;
                }

                yield {
                    id: createId(),
                    generator: 'basicObjectValueGeneratorFactory',
                    type: 'object',
                    properties: o,
                };
            }
        }
    };

    return stateAwareGenerator(configuration, checker, state, type, {
        generateEmpty,
        generate,
    });
};

const objectValueGeneratorFactory: ValueGenerator = function (configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, type: ts.Type) {
    if (type.flags !== ts.TypeFlags.Object) {
        return;
    };

    if (type.isClass()) {
        const typeName = type.getSymbol()?.getName();
        if (typeName === 'Map') {
            return mapValueGeneratorFactory(configuration, checker, state, type);
        }

        if (typeName === 'Set') {
            return setValueGeneratorFactory(configuration, checker, state, type);
        }
    }

    return basicObjectValueGeneratorFactory(configuration, checker, state, type);

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

const stupidPropertyPicker: PropertyPicker = function* (keys: string[]) {
    while (true) {
        yield keys;
    }
};

//  TODO: at some point create jq-compatible paths in pathToHere for neatness
function generatorator(configuration: GeneratorConfiguration, checker: ts.TypeChecker, state: GeneratorState, currentType: ts.Type): G {

    if (state.currentDepth > configuration.maxDepth) {
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
        const generator = factory(configuration, checker, state, currentType);
        if (generator) {
            return generator;
        }
    }

    throw new Error(`Unexpected type ${checker.typeToString(currentType)} ${checker.typeToTypeNode(currentType, undefined, undefined)?.getText()}`);
}

//  construct a stateful hierarchy of generators    
function* functionGeneratorator(checker: ts.TypeChecker, f: ts.FunctionDeclaration): G {

    const state: GeneratorState = {
        currentDepth: 0,
        pathToHere: [],
    };

    const configuration: GeneratorConfiguration = {
        maxDepth: 3,
        weirdness: 1,
    };

    const ft = checker.getTypeAtLocation(f);//  TODO: when can we directly get a ts.Type that is a function?
    console.log(`function type = ${checker.typeToString(checker.getTypeAtLocation(f))}`);

    //  don't try to convert this to the factory/generator style because function declarations require
    //  an AST Node not just a type
    const generators: Generator<GeneratedParameter, any, any>[] = [];
    for (let j = 0; j < f.parameters.length; j++) {
        const t = f.parameters[j].type;
        const currentType = t
            ? checker.getTypeAtLocation(t)
            : checker.getAnyType();

        const generator = generatorator(configuration, checker, state, currentType);
        generators.push(generator);
    }

    while (true) {
        const values: any[] = [];
        for (let i = 0; i < generators.length; i++) {
            const generator = generators[i];
            const next = generator.next();
            if (next.done) {
                throw new Error(`Generator[${i}] ${generator.constructor.name} is done`);
            }
            values.push(next.value);
        }
        yield {
            id: createId(),
            generator: 'functionGeneratorator',
            type: 'tuple',
            values,
        };
    }
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