
//  TODO: generify value
//  NOTE: all value objects must be serializable
export type GeneratedParameter = {
    id: string,
    generator: string,
    options?: Record<string, any>,
} & ({
    type: 'value',
    value: any,
} | {
    type: 'array',
    elements: GeneratedParameter[],
} | {
    type: 'tuple',
    values: GeneratedParameter[],
} | {
    type: 'intersection',
    parts: GeneratedParameter[],
} | {
    type: 'class',
    instance: any,
} | {
    type: 'map',
    entries: [GeneratedParameter, GeneratedParameter][],
} | {
    type: 'set',
    entries: GeneratedParameter[],
} | {
    type: 'date',
    epochMs: number,
} | {
    type: 'regexp',
    pattern: string,
} | {
    type: 'constructor',
    constructed: GeneratedParameter,
} | {
    type: 'callable',
    returnValue: GeneratedParameter,
} | {
    type: 'object',
        properties: Record<string, GeneratedParameter>,
    required: string[],
});

export const extractGeneratedParameterValue = (gp: GeneratedParameter): any => {
    if (gp.type === 'value') {
        return gp.value;
    }
    if (gp.type === 'array') {
        if (gp.elements) {
            return gp.elements.map(extractGeneratedParameterValue);
        }
        throw new Error(`Unexpected missing elements in array gp ${JSON.stringify(gp)}`)
    }
    if (gp.type === 'object') {
        const o: Record<string, any> = {};
        Object.entries(gp.properties).forEach(([k, v]) => {
            o[k] = extractGeneratedParameterValue(v);
        });
        return o;
    }
    if (gp.type === 'map') {
        const m = new Map();
        gp.entries.forEach(([k, v]) => {
            const key = extractGeneratedParameterValue(k);
            const value = extractGeneratedParameterValue(v);
            m.set(key, value);
        });
        return m;
    }
    if (gp.type === 'set') {
        const s = new Set();
        gp.entries.forEach((v) => {
            const value = extractGeneratedParameterValue(v);
            s.add(value);
        });
        return s;
    }
    if (gp.type === 'date') {
        return new Date(gp.epochMs);
    }
    if (gp.type === 'regexp') {
        return new RegExp(gp.pattern);
    }
    if (gp.type === 'class') {
        return gp.instance;
    }
    if (gp.type === 'tuple') {
        return gp.values.map(extractGeneratedParameterValue);
    }
    if (gp.type === 'constructor') {
        const v = extractGeneratedParameterValue(gp.constructed);
        return (_:any) => v;
    }
    if (gp.type === 'callable') {
        const v = extractGeneratedParameterValue(gp.returnValue);
        return (_:any) => {
            return v;
        };
    }
    if (gp.type === 'intersection') {
        const merged: any = {};
        for (const part of gp.parts) {
            const o = extractGeneratedParameterValue(part);
            Object.assign(merged, o);
        }
        return merged;
    }
    throw new Error(`Unexpected type ${gp['type']}`);
};

