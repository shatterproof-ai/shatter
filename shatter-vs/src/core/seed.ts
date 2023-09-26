import { faker } from '@faker-js/faker';
import { createId } from "@paralleldrive/cuid2";

export interface Literals {
    numbers: Set<number>,
    strings: Set<string>,
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
    type: 'tuple',
    values: GeneratedParameter[],
} | {
    type: 'class',
    instance: any,
} | {
    type: 'constructor',
    constructor: any,
} | {
    type: 'callable',
    callable: any,
} | {
    type: 'object',
    properties: Record<string, GeneratedParameter>,
});

const gpv = (value: number | string | boolean, generator: string, options?: Record<string, any>): GeneratedParameter & {
    type: 'value'
} => ({
    id: createId(),
    generator,
    type: 'value',
    value,
    options,
});


const numberNeighbors = [-2, -1, 0, 1, 2];

function* neighboringNumbers(n: number) {
    for (const neighbor of numberNeighbors) {
        const v = n + neighbor;
        yield v;
    }
}

function* neighboringStrings(s: string) {
    yield s;
    if (s.length === 0) {
        return;
    }
    //  remove one character at the beginning

    const rest = s.slice(1);
    yield rest;

    //  remove one character at the end
    yield s.substring(0, s.length - 1);

    //  duplicate the last character
    yield s + s.charAt(s.length - 1);

    yield ' \t' + s + ' \t';
    yield ' \t\n' + s + ' \t\n';

    const neighbors = [-2, -1, 1, 2];
    const neighboringChars = function* (s: string) {
        if (s.length !== 1) {
            throw new Error(`prevChar called on ${s} with length ${s.length} not 0`);
        }

        const code = s.codePointAt(0);
        if (code) {
            for (const offset of neighbors) {
                const targetCode = code + offset;
                if (targetCode > 31) {
                    const result = String.fromCodePoint(targetCode);
                    yield result;
                }
            }
        }
    };

    for (const pos of [0, s.length - 1]) {
        const char = s.charAt(pos);
        for (const neighbor of neighboringChars(char)) {
            const result = s.slice(0, pos) + neighbor + s.slice(pos + 1);
            yield result;
        }
    }
}

const primes = [11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73, 79, 83, 89, 97];
//  go for absolute most common and extremes    -   for SEED
const seedNumbers = [0, 1, -1, 2, 1_024, Math.PI, 4, 8, 500, 16, 25, -1_000_000, 1_000_000, 32, 40, 64, 100, Math.SQRT2, 128, 250, 256, 512, 1_000, 2048, -1_000_000_000, 1_000_000_000];
//  for BREED
const breedNumbers = (() => {
    const seen = new Set<number>(seedNumbers);
    const add = (n: number) => {
        if (!seen.has(n)) {
            seen.add(n);
        }
    };

    const bases = [[2, 63], [5, 6], [10, 10]];
    const mults = [1, -1];

    for (let i = -1; i < 11; i++) {
        if (!seen.has(i)) {
            add(i);
        }
    }

    for (const prime of primes) {
        for (const v of neighboringNumbers(prime)) {
            add(v);
        }
    }

    //  pure exponents e.g. 625, 4096, 100_000_000
    for (const mult of mults) {
        for (const [base, maxponent] of bases) {
            for (let i = 0; i < maxponent; i++) {
                const powered = mult * (base ** i);
                for (const v of neighboringNumbers(powered)) {
                    add(v);
                }
            }
        }
    }

    //  e.g. -45, 720, 250
    for (const mult of mults) {
        for (let pow2 = 1; pow2 < 10; pow2++) {
            for (let pow3 = 1; pow3 < 4; pow3++) {
                for (let pow5 = 1; pow5 < 6; pow5++) {
                    const ppow = mult * (2 ** pow2) * (3 ** pow3) * (5 ** pow5);
                    for (const v of neighboringNumbers(ppow)) {
                        add(v);
                    }
                }
            }
        }
    }

    const irrationals = [
        Math.PI,
        Math.E,
        Math.SQRT2,
        Math.LN10,
        Math.LN2,
        Math.LOG10E,
        Math.LOG2E,
        Math.SQRT1_2,
    ];

    for (const power of [1, 2, 3]) {
        for (let i = -1; i < 50; i++) {
            for (const seed of irrationals) {
                const v = i * seed ** power;
                add(v);
            }
        }

        //  pure exponents e.g. 625, 4096, 100_000_000
        for (const mult of mults) {
            for (const [base, maxponent] of bases) {
                for (let i = 0; i < maxponent; i++) {
                    for (const seed of irrationals) {
                        const powered = seed * mult * (base ** i);
                        add(powered);
                    }
                }
            }
        }

        //  e.g. likely fractions
        for (const mult of mults) {
            for (let pow2 = -3; pow2 < 4; pow2++) {
                for (let pow3 = -3; pow3 < 3; pow3++) {
                    for (let pow5 = -3; pow5 < 3; pow5++) {
                        for (let pow7 = -3; pow7 < 3; pow7++) {
                            for (const seed of irrationals) {
                                const ppow = seed * mult * (2 ** pow2) * (3 ** pow3) * (5 ** pow5) * (7 ** pow7);
                                add(ppow);
                            }
                        }
                    }
                }
            }
        }
    }

    //  arbitrary measure that's just a deterministic shuffle
    const weirdness = (n: number) => {
        return Math.log(n) % 1;
    };

    //  weirdest first
    return Array.from(seen).sort((a, b) => weirdness(b) - weirdness(b));
})();

const seedStrings = ["https://www.shatterproof.ai/en-US/support?q=testing#t39192", "zoidberg@example.com",
    "Babu Chen", "36 Church Street", "+1 802-879-7121", "#3eabef", "repurpose web-enabled e-commerce", "blob",
    "73838639", "3U32v1KXzTaES2XQ9MqapQz7hFPAQcuhpqkdQjS", "6759-5549-3524-6828-05", "HKD", "C$",
    "GR9500328930869462827058136", "544540301", "bb2bdcec", "pessimistic-chain.info", "info", "🐵", "DELETE",
    "70.248.90.36", "bdb1:8846:96cc:c5ad:1bea:ed90:d94b:18ba", "22:74:66:42:cd:a1", "w", "https",
    "http://second-hand-tremor.com/", "Mozilla/5.0 (X11; Linux x86_64; rv:11.7) Gecko/20100101 Firefox/11.7.2",
    "932", "Gerryworth", "Burkina Faso", "IM", "Cambridgeshire", "West", "Apt. 352", "ME", "Huel Terrace",
    "37859 Therese Viaduct", "Asia/Kabul", "37848-4826",
    "Distinctio commodi doloremque. Aliquam repudiandae voluptates neque quibusdam dolorum dolorum veniam. Impedit debitis vitae dolore accusamus unde temporibus ipsum aliquid fuga.\nConsequuntur deleniti eius perspiciatis hic. Delectus impedit totam iusto adipisci aliquam officiis. Laborum ab culpa eligendi dignissimos fugiat ullam quaerat.\nUllam veniam ullam. Cum esse suscipit sapiente fugit excepturi asperiores qui alias. Magni ex sint similique deserunt sint earum unde.",
    "Hermaphrodite", "National", "Mrs.", "female", "MD", "Virgo", "60-926413-577421-1", "1-395-779-3064 x60295",
    "A", "K", "0b0", "0x7", "3", "0o3", "b1abe6f0-349a-43b0-ab2f-c2a193c3a37d", "26 * ? * 4", "/proc",
    "application/vnd.mozilla.xul+xml", "ens7f7", "5.4.4", "Electric", "Mini", "KUSDX1AY6LH949957"
];

const breedStrings = ["#3eabef", "repurpose web-enabled e-commerce", "blob", "73838639",
    "3U32v1KXzTaES2XQ9MqapQz7hFPAQcuhpqkdQjS", "6759-5549-3524-6828-05", "HKD", "C$",
    "GR9500328930869462827058136", "544540301", "bb2bdcec", "d3a770e6f73bdb18", "84696ccc5ad1beaed90d94b18ba33a68",
    "863bcd1f6fcdae04cca4bce75c8f39d8a7b68e8c", "74acce7eeca8acac9c5e0f90a6ee4cee7fad26ddc48f53b5dedfaba56dfe1daa",
    "thrifty-flume.biz", "com", "🐀", "POST", "215.188.116.97", "f3cf:40d3:8cde:ced9:7b6f:ef2e:4da3:7baa",
    "74:1e:2f:8b:bf:61", "a7-a1-4f-90-92-55", "7b1abe6f0349", "g", "h", "zodaxef", "qU4P1Al", "kujicigi", "Mpv0wVSA",
    "hesaqukazirusagokudayabajuheyes", "FXjpc5u8DdsZ5MItaG7VIDrEIodTg0f", "potexukadijucobulomazuzafepuvawo",
    "eRh1oG6KBvuv4j_jyDkbodyRUF1LbdMG", "vicuborilucaqipitepunesisodusazeq", "QmZhH4ZEu3CZ6mOOXFAM0fR9bumaEc9Of",
    "totavajukadanecetowalolojapobalawahelihosudaheheridevipegozacoqum",
    "oWe9eauIIgGF3ZchA5z_SBZDMrp6SH2StU6NNjeoPmerNap0mL33Ds39OfcDuzyBN",
    "lutatuwisebufupemorewacuxutoguqafetofogocoyuxasaxazohiwihebusiduhoroganegerokopabodirugaxejekoqunequzicepufakuhefifayiyekemaruj",
    "GjwV9M0MrmHOtIAAI0DNCQO038oYDnewXFBpUupuGcsV3F2_1_If3quA2IdRljHcM3q2osL3qZm62jx8KvDSlTyo0UgQDdHjgddqBmnNwzfep4G2yPnN1Wu4bYOtrZv",
    "https", "http://vivacious-chaos.com/", "http://slimy-blackboard.org/", "http://last-urgency.net/",
    "http://reckless-politics.com/", "https://lean-dynasty.biz/", "https://precious-misreading.org/",
    "https://occasional-fluke.info/", "https://grubby-robe.name/", "http://reliable-hashtag.info",
    "http://half-deviation.net", "http://vicious-connection.name", "http://gloomy-declaration.info",
    "https://alarmed-shed.biz", "https://sweaty-committee.name", "https://high-level-strategy.biz",
    "https://powerful-flanker.biz",
    "Mozilla/5.0 (Macintosh; PPC Mac OS X 10_5_9 rv:6.0; SO) AppleWebKit/534.2.1 (KHTML, like Gecko) Version/6.0.0 Safari/534.2.1",
    "582", "North Louvenia", "Denmark", "AG", "BHS", "SZ", "Bedfordshire", "West", "Apt. 290", "VT",
    "Connecticut", "Michaela Mountains", "72786 Arianna Land", "America/Fortaleza", "15497",
    "Quaerat voluptatibus minus quibusdam ad accusantium. Sunt saepe non neque. Repudiandae vitae amet.\nDeserunt voluptatibus debitis. Debitis doloribus tempora repellat cum quo nihil porro doloribus. Eveniet mollitia laborum numquam accusantium possimus.\nQuisquam iusto molestiae. Laboriosam quisquam reiciendis autem voluptatem earum assumenda a illo. Magnam reprehenderit nulla occaecati eum.",
    "Transexual", "Customer", "Mr.", "male", "PhD", "Pisces", "37-344623-931063-8", "450-265-7117 x5515", "A", "IDOCAOX",
    "KSOPATAEZBBOYER", "MKYZQMMBTURMZJJSVXCGKYSQNLIKLHQET", "i", "geaztls", "iesblulaxccixwl", "iwskyalbibfayamokbnmvdhpzltjejvor",
    "X", "kjkZypI", "llmKcEKYwireBWT", "HJTgDBkpYFtHwKNOQEhdonVQKkcFeWnNN", "G", "VA4Z2QT", "LI5XP40AIRY79JN", "07R9612J1VIKSTUGLEJJXR9JJO3J22JUN",
    "t", "uznlblb", "t2gogkfp5vk16kz", "b193ndojg35z6ps2actvfe8twz0m6jicb", "k", "SJmTXZ4", "OIqKsvvcqwoOBSr", "GJ2o5XQmrlNLMuOLEsPaFzM3apUf2VfoP",
    "0b1", "0b1011010", "0b001100011111001", "0b110010101100000110000100011010101", "0x5", "0x2A47B77", "0x8D9F64DEAAA50FB",
    "0xFEBF6BD8CEF6D7E7ACEB11ABF1ACFAF1A", "0x2", "0xefffc1a", "0xfe6eba0e8d15e40", "0xe7a3e3a66d8537e04b8fade6ae74f2989", "0xC",
    "0xb5a7F7b", "0x552Aa62dfAB8bEE", "0xAeD20c7b2acE19509c19FEe5FadA453f8", "1", "3600510", "832438933034921", "181266784105626110570954042129309",
    "9", "5621911", "164521589773159", "392137832699355628461751026355878", "0o6", "0o5712575", "0o627737540510674",
    "0o271114714516123514146076265613234", "898f9e74-d64e-4d68-971a-58d59ff79eae", "* 19 ? * 3", "/var/log", "audio/3gpp", "enxfb0483fd2ae2",
    "wlo1", "wws1", "4.8.2", "Gasoline", "Tesla", "7VYK47S021A328481"
];

export function* edgyNumbers(literals?: Literals): Generator<GeneratedParameter, void, unknown> {
    while (true) {
        for (const m of [1, -1, 2, -2]) {
            for (const base of literals?.numbers ?? []) {
                for (const n of neighboringNumbers(base * m)) {
                    yield gpv(n, 'literals.numbers');
                }
            }
        }
        for (const n of seedNumbers) {
            yield gpv(n, 'seedNumbers');
        }
        for (const n of breedNumbers) {
            yield gpv(n, 'breedNumbers');
        }
    }
}

export function* edgyBooleans(literals?: Literals): Generator<GeneratedParameter, void, unknown> {
    while (true) {
        yield gpv(true, 'edgyBooleans');
        yield gpv(false, 'edgyBooleans');
    }
}

export const stringFakerses = {
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
        const fd = faker[domain as keyof typeof faker];
        const f = [fd[generator as keyof typeof fd]];
        if (!f) {
            throw new Error(`No faker for ${domain}.${generator}`);
        }
        dataDomains.string[`${domain}-${generator}`] = f;
    });
});

faker.seed(10481);

export function* edgyAny(literals?: Literals): Generator<GeneratedParameter, void, unknown> {
    while (true) {
        for (const n of literals?.numbers ?? []) {
            yield gpv(n, 'literals.numbers');
        }

        for (const s of literals?.strings ?? []) {
            yield gpv(s, 'literals.numbers');
        }

        yield {
            id: createId(),
            generator: 'edgyAny',
            type: 'object',
            properties: {},
        };
    }
}

export const optionVariantsLimited: Record<string, Record<string, any>> = {
    password: {
        length: [8, 24],
    },
    commitSha: {
        length: [40, 64],
    },
    countryCode: {
        variant: ['alpha-2', 'alpha-3'],
    },
    paragraph: {
        sentenceCount: [3],
    },
    alpha: {
        casing: ['mixed'],
        length: [15],
    },
    alphanumeric: {
        casing: ['mixed'],
        length: [14],
    },
    binary: {
        length: [16],
    },
    hexadecimal: {
        casing: ['upper'],
        length: [16],
    },
    numeric: {
        length: [10],
    },
};

//  TODO: merge with optionVariantsLimited
export const optionVariantsMedium: Record<string, Record<string, any>> = {
    email: {
        allowSpecialCharacters: [true, false],
    },
    mac: {
        separator: [':', '-'],
    },
    password: {
        length: [1, 7, 8, 31, 32, 33, 65, 127],
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
        sentenceCount: [1, 2, 100, 1111],
    },
    alpha: {
        casing: ['upper', 'lower', 'mixed'],
        length: [1, 7, 15, 33],
    },
    alphanumeric: {
        casing: ['upper', 'lower', 'mixed'],
        length: [1, 7, 15, 33],
    },
    binary: {
        length: [1, 7, 15, 33],
    },
    hexadecimal: {
        casing: ['upper', 'lower', 'mixed'],
        length: [1, 7, 15, 33],
    },
    numeric: {
        allowLeadingZeros: [true, false],
        length: [1, 7, 15, 33],
    },
    octal: {
        length: [1, 7, 15, 33],
    },
    networkInterface: {
        interfaceType: ['en', 'wl', 'ww'],
    },
};

//TODO: merge with optionVariantsMedium
export const optionVariantsExtensive: Record<string, Record<string, any>> = {
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

//  TODO: apply options
export function* edgyStrings(literals?: Literals): Generator<GeneratedParameter, void, unknown> {
    const seen = new Set<string>();

    for (const s of literals?.strings ?? []) {
        const sn = gpv(s, 'literals.strings');
        if (!seen.has(sn.value)) {
            yield sn;
            seen.add(sn.value);
        }
    }

    for (const s of seedStrings) {
        for (const value of neighboringStrings(s)) {
            if (!seen.has(value)) {
                seen.add(value);
                yield {
                    id: createId(),
                    generator: 'seedStrings',
                    type: 'value',
                    value,
                };
            }
        }
    }

    for (const value of breedStrings) {
        if (!seen.has(value)) {
            seen.add(value);
            yield {
                id: createId(),
                generator: 'seedStrings',
                type: 'value',
                value,
            };
        }
    }

    function* optionExplore(allVariants: any, keys: string[], chosen: any): Generator<any, any, any> {
        if (keys.length === 0) {
            yield chosen;
            return;
        }

        const currentKey = keys[0];
        const restKeys = keys.slice(1);
        //  always try one without anything selected for this one
        yield* optionExplore(allVariants, restKeys, chosen);

        if (keys.length > 0) {
            const variants = allVariants[currentKey];
            for (const variant of variants) {
                const newChoices = {
                    ...chosen,
                    [currentKey]: variant,
                };
                yield* optionExplore(allVariants, restKeys, newChoices);
            }
        }
    }

    function* optionate(fakerKey: string) {
        const optionSets = [optionVariantsLimited, optionVariantsMedium, optionVariantsExtensive];
        while (true) {
            for (const optionSet of optionSets) {
                if (optionSet[fakerKey]) {
                    const keys = Object.keys(optionSet[fakerKey]);
                    yield* optionExplore(optionSet[fakerKey], keys, {});
                } else {
                    yield {};
                }
            }
        }
    }

    while (true) {
        for (const [fakerCategoryName, fakers] of Object.entries(stringFakerses)) {
            for (const fakerName of fakers) {
                const fakerCategory = faker[fakerCategoryName as keyof typeof faker];
                const fakerFunction = fakerCategory?.[fakerName as keyof typeof fakerCategory] as any;
                if (!fakerFunction) {
                    throw new Error(`No faker for ${fakerCategoryName}.${fakerName}`);
                }

                for (const options of optionate(fakerName)) {
                    yield gpv(fakerFunction(options), `${fakerCategoryName}.${fakerName}`);
                }
            }
        }
    }
}
