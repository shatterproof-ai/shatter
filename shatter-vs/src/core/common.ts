
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
    range: GeneratedParameter[],
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
    required
});

export const extractGeneratedParameterValue = (node: GeneratedParameter): any => {
    if (node.type === 'value') {
        return node.value;
    }
    if (node.type === 'array') {
        return node.range.map(extractGeneratedParameterValue);
    }
    if (node.type === 'object') {
        const o: Record<string, any> = {};
        Object.entries(node.properties).forEach(([k, v]) => {
            o[k] = extractGeneratedParameterValue(v);
        });
        return o;
    }
    if (node.type === 'map') {
        const m = new Map();
        node.entries.forEach(([k, v]) => {
            const key = extractGeneratedParameterValue(k);
            const value = extractGeneratedParameterValue(v);
            m.set(key, value);
        });
        return m;
    }
    if (node.type === 'set') {
        const s = new Set();
        node.entries.forEach((v) => {
            const value = extractGeneratedParameterValue(v);
            s.add(value);
        });
        return s;
    }
    if (node.type === 'date') {
        return new Date(node.epochMs);
    }
    if (node.type === 'regexp') {
        return new RegExp(node.pattern);
    }
    if (node.type === 'class') {
        return node.instance;
    }
    if (node.type === 'tuple') {
        return node.values.map(extractGeneratedParameterValue);
    }
    if (node.type === 'constructor') {
        const v = extractGeneratedParameterValue(node.constructed);
        return (_:any) => v;
    }
    if (node.type === 'callable') {
        const v = extractGeneratedParameterValue(node.returnValue);
        return (_:any) => v;
    }
    if (node.type === 'intersection') {
        const merged: any = {};
        for (const part of node.parts) {
            const o = extractGeneratedParameterValue(part);
            Object.assign(merged, o);
        }
        return merged;
    }
    throw new Error(`Unexpected type ${node['type']}`);
};
