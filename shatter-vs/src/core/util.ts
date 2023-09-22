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
