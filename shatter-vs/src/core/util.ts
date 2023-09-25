import { distance as levenshtein } from 'fastest-levenshtein';

//  based on https://github.com/erdtman/canonicalize (Apache licensed)
//  for some reason the import wasn't working with the library
export function canonicallyStringify(object: any): string {
    if (typeof object === 'number' && isNaN(object)) {
        throw new Error('NaN is not allowed');
    }

    if (typeof object === 'number' && !isFinite(object)) {
        throw new Error('Infinity is not allowed');
    }

    if (object === null || typeof object !== 'object') {
        return JSON.stringify(object);
    }

    if (object.toJSON instanceof Function) {
        return canonicallyStringify(object.toJSON());
    }

    if (Array.isArray(object)) {
        const values = object.reduce((t, cv, ci) => {
            const comma = ci === 0 ? '' : ',';
            const value = cv === undefined || typeof cv === 'symbol' ? null : cv;
            return `${t}${comma}${canonicallyStringify(value)}`;
        }, '');
        return `[${values}]`;
    }

    const values = Object.keys(object).sort().reduce((t, cv) => {
        if (object[cv] === undefined ||
            typeof object[cv] === 'symbol') {
            return t;
        }
        const comma = t.length === 0 ? '' : ',';
        return `${t}${comma}${canonicallyStringify(cv)}:${canonicallyStringify(object[cv])}`;
    }, '');
    return `{${values}}`;
};


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

export function computeDistance(a: any, b: any): number {
    if (a === b || a === null || b === null || a === undefined || b === undefined) {
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
