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

const primes = [11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73, 79, 83, 89, 97];
//  go for absolute most common and extremes    -   for SEED
const seedNumbers = [0, 1, -1, 2, 1_024, Math.PI, 4, 8, 500, 16, 25, -1_000_000, 1_000_000, 32, 40, 64, 100, Math.SQRT2, 128, 250, 256, 512, 1_000, 2048, -1_000_000_000, 1_000_000_000];
//  for BREED
const breedNumbers = (() => {
    const numbers: number[] = [];
    const seen = new Set<number>(seedNumbers);
    const add = (n: number) => {
        if (!seen.has(n)) {
            numbers.push(n);
            seen.add(n);
        }
    };

    const neighbors = [-2, -1, 0, 1, 2];

    function* geneighbor(n: number, generator: string) {
        for (const neighbor of neighbors) {
            const v = n * neighbor;
            yield v;
        }
    }

    const bases = [[2, 63], [5, 6], [10, 10]];
    const mults = [1, -1];

    for (let i = -1; i < 11; i++) {
        if (!seen.has(i)) {
            add(i);
        }
    }

    for (const prime of primes) {
        for (const v of geneighbor(prime, 'primes')) {
            add(v);
        }
    }

    //  pure exponents e.g. 625, 4096, 100_000_000
    for (const mult of mults) {
        for (const [base, maxponent] of bases) {
            for (let i = 0; i < maxponent; i++) {
                const powered = mult * (base ** i);
                for (const v of geneighbor(powered, 'pureExponents')) {
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
                    for (const v of geneighbor(ppow, 'exponentProducts')) {
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

    for (const powers of [1, 2, 3]) {
        for (let i = -1; i < 50; i++) {
            for (const seed of irrationals) {
                const v = i * seed;
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

    //  arbitrary measure that's probably more like a deterministic shuffle
    const weirdness = (n: number) => {
        return Math.log(n) % 1;
    };

    //  weirdest first
    numbers.sort((a, b) => weirdness(b) - weirdness(b));
    return numbers;

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

export type Mutation = {
    path: string[],
    before: any,
    after: any,
    type: 'scramble' | 'lengthen' | 'shorten' | 'replace'
};

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

function* roundRobin(...generators: Generator<GeneratedParameter, any, any>[]) {
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

export function computeDistance(a: any, b: any): number {
    if (a === b || a === null || b === null) {
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

export class CombinatorialTestCaseSource /* implements TestCaseSource */ {

    private counter = 0;

    //  TODO: use this
    private weirdness = 1;

    //  how deep to go into nested objects; meant to be increased
    //  as more parameters are created
    private maxDepth = 3;

    private allExecutedLines = new Set<number>();

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
    5) identify unexecuted lines and try to mutate the minima to cover them
        (how to avoid just regenerating the previously attempted non-minimal values
            or ones that will be similarly ineffective?)
    6) take the minima, compare them against the other clusters and hybridize for edginess

    */

    valueGenerators = new Map<string, Generator<GeneratedParameter, any, any>>();

    *seed(): Iterator<Specimen> {
        const newGenPerPass = 10;
        const that = this;

        const edgies: Partial<Record<ts.TypeFlags, (() => Generator<GeneratedParameter, any, any>)>> = {
            [ts.TypeFlags.Any]: edgyAny,
            [ts.TypeFlags.Unknown]: edgyAny,
            [ts.TypeFlags.String]: function* () {
                for (const s of seedStrings) {
                    yield gpv(s, 'mostSpecialStrings');
                }
            },
            [ts.TypeFlags.Number]: function* () {
                for (const n of seedNumbers) {
                    yield gpv(n, 'mostSpecialNumbers');
                }
            },
            [ts.TypeFlags.Boolean]: edgyBooleans,
        };
        edgies[ts.TypeFlags.BooleanLiteral] = edgies[ts.TypeFlags.Boolean];
        edgies[ts.TypeFlags.NumberLiteral] = edgies[ts.TypeFlags.Number];
        edgies[ts.TypeFlags.StringLiteral] = edgies[ts.TypeFlags.String];
        edgies[ts.TypeFlags.StringOrNumberLiteral] = () => roundRobin(edgies[ts.TypeFlags.StringLiteral] as any, edgies[ts.TypeFlags.NumberLiteral] as any);

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
            if (currentType.isIntersection()) {
                const intersectingTypes = currentType.types;
                //  presumably must be an object type
                const o: Record<string, GeneratedParameter> = {};
                intersectingTypes.forEach((t) => {
                    const v = valueForType(checker, t, allowedDepth, pathToHere.concat('.&'));
                    if (v) {
                        Object.assign(o, v);
                    }
                });
                return {
                    id: createId(),
                    generator: 'intersector',
                    type: 'object',
                    properties: o,
                };
            }

            while (true) {
                let pathGenerator = that.valueGenerators.get(strungPath);
                if (!pathGenerator) {
                    let gengens = edgies[currentType.flags];
                    if (!gengens) {
                        if (currentType.isUnion()) {
                            const unitedTypes = currentType.types;
                            function* genUnion() {
                                for (const t of unitedTypes) {
                                    const v = valueForType(checker, t, allowedDepth, pathToHere.concat('.|'));
                                    if (v) {
                                        yield v;
                                    }
                                }
                            }

                            gengens = genUnion;
                        } else {
                            throw new Error(`Dunno how to handle type ${currentType.flags}: ${checker.typeToString(currentType)}`);
                        }
                    }

                    pathGenerator = gengens();
                    that.valueGenerators.set(strungPath, pathGenerator);
                }

                let next = pathGenerator.next();
                if (!next.done) {
                    const key = toKey(pathToHere, next.value);
                    //  in theory we want to avoid the same value in the same place repeatedly
                    //  but it's not terrible, and the whole object duplicate avoidance may be adequate
                    // if (!fqseen.has(key)) {
                    return next.value;
                    // }
                    // next = gengens[i].next();
                }
                //  restart the generator
                that.valueGenerators.delete(strungPath);
            }

            throw new Error(`Ran out of values for ${currentType.flags}: ${checker.typeToString(currentType)} at ${JSON.stringify(pathToHere)}`);
        };

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
                type: 'seed',
            };
        }
    }

    increaseWeirdness(): void {
        this.weirdness++;
    }

}