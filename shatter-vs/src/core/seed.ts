import { faker } from '@faker-js/faker';
import { ArrayGeneratedParameter, BooleanGeneratedParameter, GeneratedParameter, NumberGeneratedParameter, ObjectGeneratedParameter, ObjectPathSegment, StringGeneratedParameter, ValueGeneratedParameter, ValueSubtype, newId } from './common';
import { reverse } from 'lodash';
import ts = require('typescript');
import { RuntimeContext } from './generator';

export interface Literals {
    numbers: Set<number>,
    strings: Set<string>,
}

const gpvs = (path: ObjectPathSegment[], value: string, generator: string, options?: Record<string, any>): StringGeneratedParameter & {
    type: 'value'
} => ({
    id: newId('value'),
    generator,
    path,
    type: 'value',
    subtype: 'string',
    value,
    options,
});

const gpvb = (path: ObjectPathSegment[], value: boolean, generator: string, options?: Record<string, any>): BooleanGeneratedParameter & {
    type: 'value'
} => ({
    id: newId('value'),
    generator,
    path,
    type: 'value',
    subtype: 'boolean',
    value,
    options,
});

const gpvn = (path: ObjectPathSegment[], value: number, generator: string, options?: Record<string, any>): NumberGeneratedParameter & {
    type: 'value'
} => ({
    id: newId('value'),
    generator,
    path,
    type: 'value',
    subtype: 'number',
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

export function* edgyNumbers(rc:RuntimeContext, path: ObjectPathSegment[], literals?: Literals): Generator<GeneratedParameter, void, unknown> {
    while (true) {
        for (const m of [1, -1, 2, -2]) {
            for (const base of literals?.numbers ?? []) {
                for (const n of neighboringNumbers(base * m)) {
                    yield gpvn(path, n, 'literals.numbers');
                }
            }
        }
        for (const n of seedNumbers) {
            yield gpvn(path, n, 'seedNumbers');
        }
        for (const n of breedNumbers) {
            yield gpvn(path, n, 'breedNumbers');
        }
    }
}

const powers = (base: number, low: number, high: number) => {
    const values: number[] = [];
    for (let i = low; i < high; i++) {
        values.push(base ** i);
    }
    return values;
};

const multiples = (base: number, low: number, high: number) => {
    const values: number[] = [];
    for (let i = low; i < high; i++) {
        values.push(base * i);
    }
    return values;
};

const specialNumberRanges: number[][] = [
    primes,
    multiples(1, 0, 1),
    multiples(1, 0, 2),
    multiples(1, 0, 3),
    multiples(1, 0, 5),
    multiples(1, 0, 8),
    multiples(1, 0, 11),
    multiples(1, -1, 1),
    multiples(1, -2, 2),

    multiples(2, 0, 20),
    multiples(2, 1, 20),
    multiples(2, -2, 10),

    multiples(3, 0, 20),
    multiples(3, 1, 20),
    multiples(3, -2, 10),

    multiples(4, 0, 20),
    multiples(4, 1, 20),
    multiples(4, -2, 10),

    multiples(5, 0, 20),
    multiples(5, 1, 20),
    multiples(5, -2, 10),

    multiples(6, 0, 20),
    multiples(6, 1, 20),
    multiples(6, -2, 10),

    multiples(7, 0, 20),
    multiples(7, 1, 20),
    multiples(7, -2, 10),

    multiples(8, 0, 20),
    multiples(8, 1, 20),
    multiples(8, -2, 10),

    multiples(9, 0, 20),
    multiples(9, 1, 20),
    multiples(9, -2, 10),

    multiples(10, 0, 20),
    multiples(10, 1, 20),
    multiples(10, -2, 10),

    powers(2, 0, 34),
    powers(2, 1, 34),
    powers(2, 1, 10),

    powers(3, 0, 10),
    powers(3, 1, 10),

    powers(5, 0, 10),
    powers(5, 1, 10),

    powers(10, 0, 10),
    powers(10, 1, 10),

    multiples(2, 0, 10),
    multiples(2, 1, 10),

].flatMap((n) => [n, reverse(n)]);

const _notSpecialEnoughNumberRanges = [
    irrationals,
    seedNumbers,
    breedNumbers,
    [65, 20, 10, 45, 53, 115],
    [90, 46, 98, 102, 99, 15, 11, 127, 34, 65, 106, 130, 47, 29, 71, 63, 0],
    [12, 49, 19],
    [25, 6, 100, 100, 116, 99, 66, 82, 9, 106],
    [57, 21, 123, 71, 93, 59, 52, 53, 15, 78, 51, 66, 76],
    [104, 34, 32, 92, 11, 28, 56, 89, 101, 56, 4, 49, 39, 106],
    [51, 2, 128, 26, 97, 49, 50, 111, 44],
    [112, 30, 123, 31, 45, 14, 118, 13, 54, 21, 2, 24, 77, 10],
    [62, 40, 107, 42, 11, 118, 74, 22, 37, 65, 51, 88, 128, 8, 70, 39, 40, 53, 69, 124],
    [116, 50, 93, 120, 5, 39],
    [102, 38, 47, 121, 76],
    [105, 83, 89, 17],
    [111, 61, 89, 86, 27, 52, 112],
    [75, 95, 47, 24],
    [22, 117, 34, 19, 19, 58, 79, 108, 50, 58, 51, 21, 103, 28, 109, 14],
    [118, 86, 15, 120, 127, 13, 78, 106, 40, 103, 9, 85, 48, 47, 11, 66, 105],
    [69, 20, 18, 126, 92, 71, 93, 128, 85, 49],
    [41, 11, 131, 44, 104, 86, 81, 57, 43, 1, 117, 110, 97],
    [48, 102, 65, 50, 9, 85, 8, 75, 72, 62, 100],
    [2, 46, 14, 29, 73, 28, 43, 55, 64, 2, 15, 130],
    [107, 94, 101, 90, 31, 88, 45, 118, 62, 37, 79, 123, 102, 72, 114, 13, 107, 122, 12],
    [20, 100, 82, 110],
    [73, 19, 5, 113, 1, 122, 2, 22],
    [58, 86, 69, 32, 110, 98, 16, 85],
    [35, 91, 54, 30, 68, 30, 44, 68, 103, 109],
    [-70, 60, -92, -112, 73, -85, 99, 92, 113, -93, -128, -74, 123, -54, -38, 23],
    [55, -35, 7, -58, -24, 2, -72, -83, 32, 4, 104, -89, 34, -83, 72, -105, -67, 57, -15],
    [-67, 26, 72, 91, 43, -96, 69, 109, 32, 71, 61, -104, 106],
    [-44, -12, 108, 33, 40, -19, 10, 5, 115],
    [-97, -3, 102, 52, 14, -66, -12, 125, 56, -54],
    [-88, 3, -82, 88, -56, -55, 42, 120, 121, 2, 95, 97, -34],
    [-19, 120],
    [-55, -132, -65, 79, -74, -98],
    [110, -60],
    [48, 68, 104, -54, 31, -130, 129, 80, 95, 74, 82, 28, -7, 112, 110],
    [49.24540704678538, 94.55573710351996, 105.79472252786928, 119.38963094144783, 67.62159890623037, 52.191368614388246, 94.89103178618824, 70.23682120513371, 100.38116978702512, 27.546792524667882, 13.754311006595838, 46.951629464861064, 87.78321193161732, 111.29559871016995, 0.28986613584451215, 95.78994318253143, 4.365994832594739],
    [15.382228474976557, 67.73793784513019, 121.70606936266789, 2.8310223845419804, 85.53097560304548, 72.4653775449061, 13.22897578774257, 62.61941819624229, 132.50173568689803, 26.62730812298408, 79.36249421351182],
    [81.46467506202043, 79.16280353579145, 39.2106568320455, 75.46971723331369, 121.06286901944938, 48.764172194987474, 64.43546525722628],
    [84.19640713522425, 34.84274709724484, 9.362205376398656, 48.48137347664194, 1.114785668371849, 107.97359505722773, 33.815254546225574, 103.4515297483752, 41.42180452936819, 92.86903726085103, 98.89036808725105, 59.3838487576821, 128.09467823976794, 70.34835713031788, 42.074066029041425, 62.211120860623076, 36.95950326871027],
    [35.82351823197807, 40.90795213734072, 7.367882887575663, 117.83717990743486, 68.36084236943354],
    [70.41868115374577, 90.72052320261452, 76.05704425207882, 44.00642843426801, 123.89916589531462, 79.1877298683011, 51.15583843971979, 116.4232590619641, 37.2317824038915, 119.05192427877793, 43.14455319650148, 81.97770982702886, 20.532318308947836, 102.52547568298253, 21.841297876692636, 117.47659345550777, 26.888118720379396, 118.13860865421982],
    [10.824812570528412, 124.8542345142631, 51.91931171065521, 56.955130789645416, 107.27766275140203, 23.434754232482586, 105.57943142595359, 44.62895211015191, 72.79568288219444, 64.08383949233966, 32.33770462517424, 18.181589948337876, 66.4978642906851, 20.615311214643974, 104.61592366252391, 128.86918789345523],
    [42.49916088159774, 42.063029222211625],
    [89.31426884161071, 61.043851737131696, 53.857676123070725, 74.02266822090044, 40.42508686880644, 11.790352917717197, 63.00731184812178, 114.98871867060369, 101.65192053321265, 121.49324542803366, 65.37996331763512, 58.48684017718115, 120.42827761687768, 14.595338251084746, 120.0179594010138],
    [29.202268324415336, 118.20222348378522, 53.760738326432154, 121.77961314017232, 101.87645440138122, 114.05647821566619, 65.00910169378574, 109.7562015059621, 47.561449550579795, 33.715523921259475, 33.12821410883543],
    [35, 29.277501914104676, -75, 101.28416841719087, -80],
    [-15, 51.05339213747966, -116, 34.824229219411556, -90, 22.40236694290722],
    [123, 118.63201616579069, -105, 106.51209396713597, 129, 109.56764196614161],
    [27, 72.25799091662834],
    [-28, 12.343220367599244, -40, 123.46823794775335, -132, 28.782210652844437, -68, 35.18352059189509, 119, 93.02342263569636, -74, 25.506335616928105, -32, 34.31557713948207, 104, 125.2010058657754, 9, 23.20149974437677, -92],
    [2, 131.92944449766816, 47, 57.24384009554342, -43, 72.16812839218518, 3, 36.90028148455498, -79, 71.15300630157229, -63, 87.85562578669172, -17, 131.37730807982354, 87, 121.50552728687661, -119, 60.02040960175827, -65, 87.44629765248804],
    [13, 92.0529978586698, 92],
    [125, 2.359100728134798, -55, 63.3961271550478, -124, 81.70561432752184, 71, 126.4135188571788, -80, 39.881554610685264],
    [86, 70.8316082510735, -106, 45.35450160817318, -58, 45.234212278565465, -14, 6.287976843729201, 126, 50.73272855912405, -56],
    [-111, 93.872878218907, -131, 87.5793543185275, 100, 57.659421572738, -24, 55.89106850879004, -42, 20.477776436872794, 68, 42.857431988066196, -114, 13.370383869929155, -112],
];

export function* edgyNumberRanges(checker: ts.TypeChecker, path: ObjectPathSegment[], literals?: Literals): Generator<GeneratedParameter, void, unknown> {
    const combinedNumberRanges = literals?.numbers
        ? [Array.from(literals?.numbers)].concat(specialNumberRanges)
        : specialNumberRanges;

    const elementPath = path.concat({
        generator: 'arrayValueGenerator-specialNumber',
        typeString: 'number',
        segment: '[]',
    });

    for (const range of combinedNumberRanges) {
        const gp: ArrayGeneratedParameter = {
            id: newId('edgy-range'),
            generator: 'arrayValueGenerator',
            type: 'array',
            elements: range.map((n) => ({
                id: newId('edgy-range-element'),
                generator: 'arrayValueGenerator-specialNumber',
                type: 'value',
                path: elementPath,
                subtype: 'number',
                value: n,
            })),
            path,
        };
        yield gp;
    }
}

export function* edgyBooleans(path: ObjectPathSegment[], literals?: Literals): Generator<GeneratedParameter, void, unknown> {
    while (true) {
        yield gpvb(path, true, 'edgyBooleans');
        yield gpvb(path, false, 'edgyBooleans');
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

export function* edgyAny(path: ObjectPathSegment[], literals?: Literals): Generator<GeneratedParameter, void, unknown> {
    while (true) {
        for (const n of literals?.numbers ?? []) {
            yield gpvn(path, n, 'literals.numbers');
        }

        for (const s of literals?.strings ?? []) {
            yield gpvs(path, s, 'literals.strings');
        }

        const ogp: ObjectGeneratedParameter = {
            id: newId('edgy-any'),
            generator: 'edgyAny',
            path,
            declaredType: 'any',
            type: 'object',
            properties: {},
            required: [],
        };
        yield ogp;
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
export function* edgyStrings(rc:RuntimeContext, path: ObjectPathSegment[], literals?: Literals): Generator<GeneratedParameter, void, unknown> {
    const seen = new Set<string>();

    rc.weirdness;

    for (const s of literals?.strings ?? []) {
        const sn = gpvs(path, s, 'literals.strings');
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
                    id: newId('edgy-strings'),
                    generator: 'seedStrings',
                    type: 'value',
                    path,
                    subtype: 'string',
                    value,
                };
            }
        }
    }

    for (const value of breedStrings) {
        if (!seen.has(value)) {
            seen.add(value);
            yield {
                id: newId('edgy-strings'),
                generator: 'seedStrings',
                type: 'value',
                path,
                subtype: 'string',
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
                    yield gpvs(path, fakerFunction(options), `${fakerCategoryName}.${fakerName}`);
                }
            }
        }
    }
}
