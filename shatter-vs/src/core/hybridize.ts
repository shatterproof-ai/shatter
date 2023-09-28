import { create, isEqual } from 'lodash';
import { GeneratedParameter, newId, skip } from './common';
import { G } from './generator';

//  TODO: split this into an initial entrypoint and a recursive internal entrypoint
export function* hybridize(a: GeneratedParameter, b: GeneratedParameter): G {
    //  stupid sort so they can be written in order but are executed from the middle out
    // const splitIntervals = [0.01, 0.1, 0.25, 0.5, 0.75, 0.9, 0.99].sort((a, b) => Math.abs(a - 0.5) - Math.abs(b - 0.5));
    const splitIntervals = [0.1, 0.5, 0.9];

    if (a.type !== b.type) {
        //  TODO: will this error be a problem with union types?
        throw new Error(`Attempting unnatural hybridization of ${a.type} with ${b.type}`);
    }

    if (a.type === 'callable' || a.type === 'constructor' || a.type === 'intersection'
        || a.type === 'regexp' || a.type === 'class' || a.type === 'tuple') {
            //  in theory regexps, classes, and tuples can be hybridized... later
        return;
    }
    if (a.type === 'value' && b.type === 'value') {
        if (a.value === undefined || a.value === null || b.value === undefined || b.value === null) {
            yield a;
            yield b;
            return;
        }

        if (typeof a.value !== typeof b.value) {
            //  TODO: will this error be a problem with union types?
            throw new Error(`Attempting unnatural hybridization of ${typeof a.value} and ${typeof b.value}`);
        }

        if (typeof a === "boolean") {
            //  no in between
            return;
        }

        if (typeof a.value === "number") {
            for (const n of hybridizeNumbers(a.value, b.value, splitIntervals)) {
                yield {
                    id: newId('hybrid-number'),
                    type: 'value',
                    generator: 'hybridize',
                    value: n,
                };
            }
            return;
        }

        if (typeof a.value === "string") {
            for (const s of hybridizeStrings(a.value, b.value, splitIntervals)) {
                yield {
                    id: newId('hybrid-string'),
                    type: 'value',
                    generator: 'hybridize',
                    value: s,
                };
            }
            return;
        }
    }

    if (a.type === 'date' && b.type === 'date') {
        if (a.epochMs !== b.epochMs) {
            yield {
                id: newId('hybrid-date'),
                type: 'date',
                generator: 'hybridize',
                epochMs: Math.floor((a.epochMs + b.epochMs)/2),
            };
        }

        return;
    }

    if (a.type === 'array' && b.type === 'array') {
        for (const elements of hybridizeArrays(a.elements, b.elements, splitIntervals)) {
            yield {
                id: newId('hybrid-array'),
                type: 'array',
                generator: 'hybridize',
                elements,
            };
        }
        return;
    }

    if (a.type === "object" && b.type === "object") {
        if (!isEqual(a.required, b.required)) {
            //  TODO: not entirely sure what to do here; seems like this should never happen because required should be immutable
            return;
        }

        for (const properties of hybridizeObjects(a.properties, b.properties, splitIntervals)) {
            yield {
                id: newId('hybrid-object'),
                type: 'object',
                generator: 'hybridize',
                properties,
                required: a.required,
            };
        }
        return;
    }


    if (a.type === 'set' && b.type === 'set') {
        const aaa = a.entries;
        const bbb = b.entries;
        for (const hhhh of hybridizeArrays(aaa, bbb, splitIntervals)) {
            yield {
                id: newId('hybrid-set'),
                type: 'set',
                generator: 'hybridize',
                entries: hhhh,
            };
        }
    }

    if (a.type === 'map' && b.type === 'map') {
        const aaa = a.entries;
        const bbb = b.entries;
        for (const hhhh of hybridizeArrays(aaa, bbb, splitIntervals)) {
            yield {
                id: newId('hybrid-map'),
                type: 'map',
                generator: 'hybridize',
                entries: hhhh,
            };
        }
    }

    throw new Error(`Unhandled input types ${a.type} from ${a.generator}:${a.id} and ${b.type} from ${b.generator}:${b.id}`);
}

function* hybridizeNumbers(a: number, b: number, intervals: number[]) {
    const larger = Math.max(a, b);
    const smaller = Math.min(a, b);
    const diff = larger - smaller;
    const seen = new Set<number>();

    //  TODO: tunable precision
    const minDiff = 0.2;

    for (const interval of intervals) {
        const value = smaller + diff * interval;
        const floored = Math.floor(value);
        const ceiled = Math.ceil(value);
        const values = new Set([value, floored, ceiled]);
        if (Number.isInteger(smaller) || Number.isInteger(larger)) {
            values.add(floored);
            values.add(ceiled);
        }

        for (const value of values) {
            if (Math.abs(value - smaller) > minDiff
                && Math.abs(value - larger) > minDiff
                && !seen.has(value)) {
                seen.add(value);
                yield value;
            }
        }
    }
}
function* hybridizeStrings(a: string, b: string, intervals: number[]) {
    //  first part is shorter string, second part is longer string
    //  first part is subset of shorter string, remainder is from longer string
    //  first part is subset of longer string, rest of shorter string, rest of longer string

    const [shorter, longer] = a.length < b.length ? [a, b] : [b, a];

    const seen = new Set<string>();
    const longerBeginning = longer.substring(0, shorter.length);
    const shorterIsSubset = longer.startsWith(shorter);
    if (!shorterIsSubset) {
        if (shorter.length < longer.length) {
            //  longer truncated to the length of shorter
            seen.add(longerBeginning);
            yield longerBeginning;

            //  longer truncated to the length of shorter - 1
            const andAgain = longerBeginning.substring(0, longerBeginning.length - 1);
            seen.add(andAgain);
            yield andAgain;
        }

        const edits = computeEdits(shorter, longer);
        //  apply some subset of edits for intermediate strings between shorter and longer
        for (const splitInterval of intervals) {
            const editsToApply = Math.floor(splitInterval * edits.length);
            if (editsToApply === 0 || editsToApply === edits.length) {
                continue;
            }

            let value: string = shorter;
            for (let i = 0; i < editsToApply; i++) {
                const edit = edits[i];
                if (edit.type === 'delete') {
                    value = value.slice(0, edit.index) + value.slice(edits[i].index + 1);
                } else if (edits[i].type === 'insert') {
                    value = value.slice(0, edit.index) + edit.value + value.slice(edits[i].index);
                } else {
                    value = value.slice(0, edit.index) + edit.value + value.slice(edits[i].index + 1);
                }
            }

            if (!seen.has(value)) {
                seen.add(value);
                yield value;
            }
        }
    }

    for (const interval of intervals) {
        //  shorter + various lengths of the rest of longer
        const toTake = Math.floor(interval * (longer.length - shorter.length));
        const remainder = longer.substring(shorter.length, shorter.length + toTake);
        const value = shorter + remainder;
        if (!seen.has(value)) {
            seen.add(value);
            yield value;
        }
        //  substrings of longer
        if (!shorterIsSubset) {
            const otherValue = longerBeginning + remainder;
            if (!seen.has(otherValue)) {
                seen.add(otherValue);
                yield otherValue;
            }
        }
    }
}

const pickHybrid = (a: any, b: any, interval: number) => {
    const hg = hybridize(a, b);
    const all = Array.from(hg);
    if (all.length > 0) {
        return all[Math.floor(all.length * interval)];
    }
};

function* hybridizeArrays(a: any[], b: any[], intervals: number[]) {
    const shorter = a.length < b.length ? a : b;
    const longer = a.length < b.length ? b : a;

    for (const interval of intervals) {
        const arr: any[] = [];
        for (let i = 0; i < shorter.length; i++) {
            // Taking only median hybrid for simplicity
            const h = pickHybrid(shorter[i], longer[i], interval);
            if (h !== undefined) {
                arr.push(h);
            }
        }
        //  add a subset of the rest of longer
        const toTake = Math.floor(interval * (longer.length - shorter.length));
        for (let i = shorter.length; i < shorter.length + toTake; i++) {
            arr.push(longer[i]);
        }
        yield arr;
    }
}

function* hybridizeObjects(a: any, b: any, intervals: number[]) {
    const numberToGenerate = intervals.length;

    const commonKeys = Object.keys(a).filter(k => b.hasOwnProperty(k));
    const distinctKeysA = Object.keys(a).filter(k => !b.hasOwnProperty(k));
    const distinctKeysB = Object.keys(b).filter(k => !a.hasOwnProperty(k));

    for (const interval of intervals) {
        const base: any = {};

        const aKeysToTake = Math.floor(interval * commonKeys.length);
        let aKeyCount = 0;
        for (const key of distinctKeysA) {
            base[key] = a[key];
            aKeyCount++;
            if (aKeyCount >= aKeysToTake) {
                break;
            }
        }

        const bKeysToTake = Math.floor((1 - interval) * commonKeys.length);
        for (const key of distinctKeysB) {
            base[key] = b[key];
            if (Object.keys(base).length === aKeysToTake + bKeysToTake) {
                break;
            }
        }

        for (let i = 0; i < numberToGenerate; i++) {
            const candidate: Record<string | number, any> = { ...base };
            for (const key of commonKeys) {
                candidate[key] = pickHybrid(a[key], b[key], interval);
            }
            yield candidate;
        }
    }
}

/**
 * 
 * to be a strict extension, the following must be true:
 * * all keys in base must be in maybeExtension
 * * all values in base must be equal to the corresponding value in maybeExtension
 *      OR the corresponding value in maybeExtension must be a strict extension of the corresponding value in base
 * 
 */
export function isStrictExtension(base: any, maybeExtension: any): boolean {
    //  nothing and nothing or nothing and something are both strict extensions
    if (base === undefined || base === null) {
        return true;
    }

    if (typeof base !== typeof maybeExtension) {
        return false;
    }

    if (typeof base === "boolean") {
        return base === maybeExtension;
    }

    if (typeof base === "number") {
        return maybeExtension === base;
    }

    if (typeof base === "string") {
        return maybeExtension.startsWith(base);
    }

    if (Array.isArray(base)) {
        if (!Array.isArray(maybeExtension)) {
            return false;
        }
        if (base.length > maybeExtension.length) {
            return false;
        }
        for (let i = 0; i < base.length; i++) {
            if (!isStrictExtension(base[i], maybeExtension[i])) {
                return false;
            }
        }
        return true;
    }

    if (typeof base === "object") {
        if (typeof maybeExtension !== "object") {
            return false;
        }
        const baseKeys = Object.keys(base);
        const maybeExtensionKeys = Object.keys(maybeExtension);
        if (baseKeys.length > maybeExtensionKeys.length) {
            return false;
        }
        for (const key of baseKeys) {
            if (!maybeExtension.hasOwnProperty(key)) {
                return false;
            }
            if (!isStrictExtension(base[key], maybeExtension[key])) {
                return false;
            }
        }
        return true;
    }

    return true;
}

/*
 compare two objects and evaluate which is more minimal
 minimal means
 * false < true
 * numbers closer to zero are better than numbers further from zero
 * shorter strings are better than longer strings
*/
function compareMinimality(a: any, b: any) {
    if (a === undefined || a === null) {
        return -1;
    }
    if (b === undefined || b === null) {
        return 1;
    }
    if (typeof a === "boolean") {
        if (typeof b === "boolean") {
            //  arbitrarily decide that false is smaller than true
            if (a === b) {
                return 0;
            }
            if (a) {
                return 1;
            }
        }
        return -1;
    }
    if (typeof b === "boolean") {
        return 1;
    }

    if (typeof a === "number") {
        if (typeof b === "number") {
            const absa = Math.abs(a);
            const absb = Math.abs(b);
            if (absa === absb) {
                //  negative numbers are less minimal than positive
                //  a = -4, b = 3 => 3 / -1
                //  a = -4, b = -3 => -3 / -1
                //  a = -4, b = 4 => 4 / 8
                //  a = -4, b = -4 => -4 / 0
                //  a = -4, b = 5 => -4 / 9
                //  a = -4, b = -5 => -4 / 1
                /**
                 * a < b => -1
                 * a == b => 0
                 * a > b => 1
                 */
                if (a < 0 && b > 0) {
                    return 1;
                }
                if (a > 0 && b < 0) {
                    return -1;
                }
                return 0;
            }
        }
        return -1;
    }

    if (typeof b === "number") {
        return 1;
    }

    if (typeof a === "string") {
        if (typeof b === "string") {
            if (a.length === b.length) {
                return 0;
            }
            if (a.length > b.length) {
                return 1;
            }
        }
        return -1;
    }
    if (typeof b === "string") {
        return 1;
    }

}

const shrinkArray = (elements: GeneratedParameter[]) => {
    if (elements.length === 0) {
        return [];
    }

    //  try with just the last element removed
    const arrayses: any[][] = [];
    arrayses.push(elements.slice(0, elements.length - 1));
    const duped = [...elements];
    for (const shrunkElement of shrink(elements[0])) {
        //  TODO: verify that this equality test doesn't have false positives
        //  and has few false negatives
        if (!isEqual(elements[0], shrunkElement)) {
            duped[0] = shrunkElement;
            //  try with the first element shrunk and the last element removed
            arrayses.push(duped.slice(0, duped.length - 1));
            //  try with the first element shrunk and everything else the same
            arrayses.push(duped);
            //  try it with just the first, shrunk element
            arrayses.push([shrunkElement]);
        }
    }
    return arrayses;
};

//  TODO: front load more dramatic shrinkings and expect the in-between to be handled by hybridization
export function* shrink(gp: GeneratedParameter): G {
    if (gp === undefined || gp === null) {
        throw new Error("Cannot shrink undefined or null");
    }

    if (gp.type === "callable" || gp.type === "constructor") {
        return;
    }

    if (gp.type === "intersection") {
        for (const part of gp.parts) {
            let i = 0;
            for (const shrunk of shrink(part)) {
                yield shrunk;
                //  arbitrary cutoff
                if (i++ > 3) {
                    break;
                }
            }
        }
        return;
    }

    if (gp.type === 'map') {
        if (gp.entries.length === 0) {
            return;
        }

        //  try without the entry at i
        for (let i = 0; i < gp.entries.length; i++) {
            yield {
                ...gp,
                entries: gp.entries.slice(0, i).concat(gp.entries.slice(i + 1)),
            };
        }

        const shrinkers: [G, G][] = [];
        for (let i = 0; i < gp.entries.length; i++) {
            shrinkers.push([
                shrink(gp.entries[i][0]),
                shrink(gp.entries[i][1]),
            ]);
        }

        //  try with every entry shrunk
        //  arbitrarily set a limit on the number of variations
        for (let i = 0; i < 5; i++) {
            const shrunkEntries: [GeneratedParameter, GeneratedParameter][] = [];
            for (let j = 0; j < gp.entries.length; j++) {
                //  skip ahead unevenly to avoid lockstep equals
                //  provides more variety, and that can be hybridized
                const shrunkKey = skip(shrinkers[j][0], 2 * i + 1);
                const shrunkValue = skip(shrinkers[j][1], 2 * i);
                if (shrunkKey && shrunkValue) {
                    shrunkEntries.push([shrunkKey, shrunkValue]);
                } else {
                    //  if we run out of shrunk values, just use the original
                    shrunkEntries.push(gp.entries[j]);
                }
            }

            yield {
                ...gp,
                entries: shrunkEntries,
            };
        }
        return;
    }

    if (gp.type === 'set') {
        if (gp.entries.length === 0) {
            return;
        }

        for (const variation of shrinkArray(gp.entries)) {
            yield {
                id: newId('shrink-set'),
                generator: 'shrinker',
                type: 'set',
                entries: variation,
            };
        }
        return;
    }

    if (gp.type === "regexp") {
        //  maybe eventually do something silly like getting the parse tree and lopping off a branch
        return;
    }

    if (gp.type === "date") {
        //  TODO: maybe there's some semantically meaningful
        //  to shrink this; maybe converge on Date.now() or nearabouts?
        return;
    }

    if (gp.type === "tuple") {
        for (const values of shrinkArray(gp.values)) {
            yield {
                id: newId('shrink-tuple'),
                generator: 'shrinker',
                type: 'tuple',
                values,
            };
        }
        return;
    }

    if (gp.type === 'value') {
        //  unshrinkable
        if (typeof gp.value === "boolean") {
            return;
        }

        if (typeof gp.value === "number") {
            //  TODO: tunable precision
            const values: number[] = [];
            if (Math.abs(gp.value) < 0.01) {
                if (gp.value === 0) {
                    return;
                }
                values.push(0);
            }

            if (gp.value !== 0) {
                values.push(gp.value / 2);

                if (gp.value > 0) {
                    values.push(Math.floor(gp.value / 2));
                } else {
                    values.push(Math.ceil(gp.value / 2));
                }
            }

            for (const v of values) {
                yield {
                    id: newId('shrink-number'),
                    generator: 'shrinker',
                    type: 'value',
                    value: v,
                };
            }

            return;
        }

        if (typeof gp.value === "string") {
            if (gp.value.length > 0) {
                yield {
                    id: newId('shrink-string'),
                    generator: 'shrinker',
                    type: 'value',
                    value: gp.value.substring(0, gp.value.length / 2),
                };
            }
            return;
        }
    }


    if (gp.type === 'array') {
        for (const array of shrinkArray(gp.elements)) {
            yield {
                id: newId('shrink-array'),
                generator: 'shrinker',
                type: 'array',
                elements: array,
            };
        }

        return;
    }

    if (gp.type === "object") {
        const keys = Object.keys(gp);
        if (keys.length === 0) {
            return;
        }

        for (const currentKey of keys) {
            //  try with the given key shrunk
            const preshrunkElement = gp.properties[currentKey];
            if (!preshrunkElement) {
                continue;
            }
            for (const shrunkElement of shrink(preshrunkElement)) {
                if (!isEqual(shrunkElement, preshrunkElement)) {
                    const shrunked: GeneratedParameter = {
                        ...gp,
                        properties: { ...gp.properties },
                    };
                    if (shrunkElement !== undefined || !gp.required.includes(currentKey)) {
                        shrunked.properties[currentKey] = shrunkElement;
                    }
                    yield shrunked;
                }
            }
            if (!gp.required.includes(currentKey)) {
                //  try with the given key removed
                const trunked: GeneratedParameter = {
                    ...gp,
                    properties: { ...gp.properties },
                };
                delete trunked.properties[currentKey];
                yield trunked;
            }
        }
        return;
    }

    throw new Error(`Unhandled input type ${gp.type}`);
}

type Edit = {
    type: 'delete'
    index: number
} | {
    type: 'insert'
    index: number
    value: string
} | {
    type: 'substitute'
    index: number
    value: string
};

const computeEdits = (start: string, end: string) => {
    const distances = [];
    for (let i = 0; i <= start.length; ++i) {
        distances[i] = [i];
    }

    for (let i = 0; i <= end.length; ++i) {
        distances[0][i] = i;
    }

    const edits: Edit[] = [];
    for (let indexInEnd = 1; indexInEnd <= end.length; indexInEnd++) {
        for (let indexInStart = 1; indexInStart <= start.length; indexInStart++) {
            const samesies = start[indexInStart - 1] === end[indexInEnd - 1];
            if (samesies) {
                const previousDifference = distances[indexInStart - 1][indexInEnd - 1];
                distances[indexInStart][indexInEnd] = previousDifference;
            } else {
                const deletion = distances[indexInStart - 1][indexInEnd] + 1;
                const insertion = distances[indexInStart][indexInEnd - 1] + 1;
                const substitution = distances[indexInStart - 1][indexInEnd - 1] + 1;

                const minned = Math.min(deletion, insertion, substitution);

                if (minned === deletion) {
                    edits.push({
                        type: 'delete',
                        index: indexInStart - 1
                    });
                } else if (minned === insertion) {
                    edits.push({
                        type: 'insert',
                        index: indexInStart - 1,
                        value: end[indexInEnd - 1]
                    });
                } else {
                    edits.push({
                        type: 'substitute',
                        index: indexInStart - 1,
                        value: end[indexInEnd - 1]
                    });
                }

                distances[indexInStart][indexInEnd] = minned;
            }
        }
    }
    return edits;
};
