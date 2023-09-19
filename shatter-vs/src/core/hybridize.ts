import { hy } from "@faker-js/faker";

function* hybridize(a: any, b: any) {
    const splitIntervals = [0.01, 0.1, 0.25, 0.5, 0.75, 0.9, 0.99];

    if (typeof a !== typeof b) {
        throw new Error("Inputs should be of the same type");
    }

    if (typeof a === "number") {
        for (const n of hybridizeNumbers(a, b, splitIntervals)) {
            yield n;
        }
    }

    if (typeof a === "string") {
        for (const s of hybridizeStrings(a, b, splitIntervals)) {
            yield s;
        }
    }

    if (typeof a === "boolean") {
        if (a || b) {
            yield true;
        }
        if (!a || !b) {
            yield false;
        }
    }

    if (Array.isArray(a) && Array.isArray(b)) {
        for (const arr of hybridizeArrays(a, b, splitIntervals)) {
            yield arr;
        }
    }

    if (typeof a === "object" && typeof b === "object") {
        for (const o of hybridizeObjects(a, b, splitIntervals)) {
            yield o;
        }
    }

    throw new Error("Unhandled input types");
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

    const remainders = Array.from(new Set(["", //  none
        longer.substring(shorter.length, 1),    //  one character
        longer.substring(shorter.length, shorter.length + (longer.length - shorter.length)),    //  half of the difference
        longer.substring(shorter.length),   //  all of the difference
    ]));

    const seen = new Set<string>();
    const longerBeginning = longer.substring(0, shorter.length);
    const shorterIsSubset = longer.startsWith(shorter);
    if (!shorterIsSubset) {
        const edits = computeEdits(shorter, longer);
        //  apply some subset of edits for intermediate strings between shorter and longer
        for (const splitInterval of intervals) {
            const editsToApply = Math.floor(splitInterval * edits.length)
            if (editsToApply == 0 || editsToApply == edits.length) {
                continue
            }

            let value: string = shorter
            for (let i = 0; i < editsToApply; i++) {
                const edit = edits[i]
                if (edit.type == 'delete') {
                    value = value.slice(0, edit.index) + value.slice(edits[i].index + 1)
                } else if (edits[i].type == 'insert') {
                    value = value.slice(0, edit.index) + edit.value + value.slice(edits[i].index)
                } else {
                    value = value.slice(0, edit.index) + edit.value + value.slice(edits[i].index + 1)
                }
            }

            if (!seen.has(value)) {
                seen.add(value)
            }
        }
    }

    for (const remainder of remainders) {
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

function* hybridizeArrays(a: any[], b: any[], intervals: number[]) {
    const minLength = Math.min(a.length, b.length);
    const maxLength = Math.max(a.length, b.length);

    const shorter = a.length < b.length ? a : b;
    const longer = a.length < b.length ? b : a;

    for (const interval of intervals) {
        const arr:any[] = [];
        for (let i = 0; i < minLength; i++) {
            arr.push(hybridize(shorter[i], longer[i]));  // Taking the first hybrid for simplicity
        }
        for (let i = minLength; i < minLength * interval * maxLength; i++) {
            arr.push(hybridize(shorter[i], longer[i]));  // Taking the first hybrid for simplicity
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
    
        const bKeysToTake = Math.floor((1-interval) * commonKeys.length);
        for (const key of distinctKeysB) {
            base[key] = b[key];
            if (Object.keys(base).length == aKeysToTake + bKeysToTake) {
                break;
            }
        }
        
        const used:Record<string|number, any[]> = {};
        const hybridizers:Record<string|number, Generator<any, any, any>> = {};    
        for (const key of commonKeys) {
            hybridizers[key] = hybridize(a[key], b[key]);
        }
        
        for (let i = 0; i < numberToGenerate; i++) {
            const candidate:Record<string|number, any> = {...base};
            for (const key of commonKeys) {
                const hk = hybridizers[key];
                const n = hk.next();
                if (n.done) {
                    hybridizers[key] = hybridize(a[key], b[key]);
                }
                candidate[key] = n.value;
            }
            yield candidate;    
        }

    }
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
}

const computeEdits = (start: string, end: string) => {
    const distances = []
    for (let i = 0; i <= start.length; ++i) {
        distances[i] = [i]
    }

    for (let i = 0; i <= end.length; ++i) {
        distances[0][i] = i
    }

    const edits: Edit[] = []
    for (let indexInEnd = 1; indexInEnd <= end.length; indexInEnd++) {
        for (let indexInStart = 1; indexInStart <= start.length; indexInStart++) {
            const samesies = start[indexInStart - 1] === end[indexInEnd - 1]
            if (samesies) {
                const previousDifference = distances[indexInStart - 1][indexInEnd - 1]
                distances[indexInStart][indexInEnd] = previousDifference
            } else {
                const deletion = distances[indexInStart - 1][indexInEnd] + 1
                const insertion = distances[indexInStart][indexInEnd - 1] + 1
                const substitution = distances[indexInStart - 1][indexInEnd - 1] + 1

                const minned = Math.min(deletion, insertion, substitution)

                if (minned == deletion) {
                    edits.push({
                        type: 'delete',
                        index: indexInStart - 1
                    })
                } else if (minned == insertion) {
                    edits.push({
                        type: 'insert',
                        index: indexInStart - 1,
                        value: end[indexInEnd - 1]
                    })
                } else {
                    edits.push({
                        type: 'substitute',
                        index: indexInStart - 1,
                        value: end[indexInEnd - 1]
                    })
                }

                distances[indexInStart][indexInEnd] = minned
            }
        }
    }
    return edits
}
