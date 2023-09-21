import { isEqual } from 'lodash';

//  TODO: split this into an initial entrypoint and a recursive internal entrypoint
export function* hybridize(a: any, b: any) {
    //  stupid sort so they can be written in order but are executed from the middle out
    const splitIntervals = [0.01, 0.1, 0.25, 0.5, 0.75, 0.9, 0.99].sort((a, b) => Math.abs(a - 0.5) - Math.abs(b - 0.5));

    if (a === undefined || a === null || b === undefined || b === null) {
        yield a;
        yield b;
        return;
    }

    if (typeof a !== typeof b) {
        throw new Error(`Differing input types ${typeof a} and ${typeof b}`);
    }

    if (typeof a === "number") {
        for (const n of hybridizeNumbers(a, b, splitIntervals)) {
            yield n;
        }
        return;
    }

    if (typeof a === "string") {
        for (const s of hybridizeStrings(a, b, splitIntervals)) {
            yield s;
        }
        return;
    }

    if (typeof a === "boolean") {
        if (a || b) {
            yield true;
        }
        if (!a || !b) {
            yield false;
        }
        return;
    }

    if (Array.isArray(a) && Array.isArray(b)) {
        for (const arr of hybridizeArrays(a, b, splitIntervals)) {
            yield arr;
        }
        return;
    }

    if (typeof a === "object" && typeof b === "object") {
        for (const o of hybridizeObjects(a, b, splitIntervals)) {
            yield o;
        }
        return;
    }

    throw new Error(`Unhandled input types ${typeof a} and ${typeof b}`);
}

function* hybridizeNumbers(a: number, b: number, intervals: number[]) {
    const larger = Math.max(a, b);
    const smaller = Math.min(a, b);
    const diff = larger - smaller;
    const seen = new Set<number>();
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
            if (!seen.has(value)) {
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

    const shorter = a.length < b.length ? a : b;
    const longer = a.length < b.length ? b : a;

    const seen = new Set<string>();
    const longerBeginning = longer.substring(0, shorter.length);
    const shorterIsSubset = longer.startsWith(shorter);
    if (!shorterIsSubset) {
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
            }
        }
    }

    for (const interval of intervals) {
        const toTake = Math.floor(interval * (longer.length - shorter.length));
        const remainder = longer.substring(shorter.length, shorter.length + toTake);
        const value = shorter + remainder;
        if (!seen.has(value)) {
            seen.add(value);
            yield value;
        }
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

        const used: Record<string | number, any[]> = {};
        const hybridizers: Record<string | number, Generator<any, any, any>> = {};
        for (const key of commonKeys) {
            hybridizers[key] = pickHybrid(a[key], b[key], interval);
        }

        for (let i = 0; i < numberToGenerate; i++) {
            const candidate: Record<string | number, any> = { ...base };
            for (const key of commonKeys) {
                const hk = hybridizers[key];
                const n = hk.next();
                if (n.done) {
                    hybridizers[key] = pickHybrid(a[key], b[key], interval);
                }
                candidate[key] = n.value;
            }
            yield candidate;
        }

    }
}

export function* shrink(o: any) {
    if (o === undefined || o === null) {
        return;
    }

    if (typeof o === "boolean") {
        return;
    }

    if (typeof o === "number") {
        if (o === 0) {
            return;
        }
        yield o/2;
        if (o > 0) {
            yield Math.floor(o/2);
        } else {
            yield Math.ceil(o/2);
        }
        return;
    }

    if (typeof o === "string") {
        if (o.length > 0) {
            yield o.substring(0, o.length/2);
        }
        return;
    }

    if (Array.isArray(o)) {
        if (o.length === 0) {
            return;
        }

        //  try with just the last element removed
        yield o.slice(0, o.length - 1);
        //  try with the first element shrunk and the last element removed
        const duped = [...o];
        duped[0] = shrink(o[0]);

        //  TODO: verify that this equality test doesn't have false positives
        //  and has few false negatives
        if (! isEqual(o[0], duped[0])) {
            yield duped.slice(0, duped.length - 1);
            //  try with just the first element shrunk
            yield duped;
        }

        return;
    }

    if (typeof o === "object") {
        const keys = Object.keys(o);
        if (keys.length === 0) {
            return;
        }

        for (let i = 0; i < keys.length; i++) {
            //  try with the given key shrunk
            const shrunked = { ...o };
            const preshrunkElement = o[keys[0]];
            const shrunkElement = shrink(preshrunkElement);
            if (! isEqual(shrunkElement, preshrunkElement)) {
                shrunked[keys[0]] = shrunkElement;
                yield shrunked;
            }
            //  try with the given key removed
            const trunked = { ...o };
            delete trunked[keys[keys.length - 1]];
            yield trunked;
        }
        return;
    }

    throw new Error(`Unhandled input type ${typeof o}`);
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
