import { GeneratedParameter, resolveGeneratedParameterValue } from "../core/common";


describe('resolveGeneratedParameterValue', () => {
    it('should return a unique UUID when generator is uuid', () => {
        const parameter:GeneratedParameter = {
            type: 'value',
            id: 'zork',
            path: [],
            generator: 'uuid',
            subtype: 'string',
            value: 'uuid',
        };
        const resolvedValue = resolveGeneratedParameterValue(parameter, false, module.exports);
        expect(typeof resolvedValue).toBe('string');
        expect(resolvedValue).toHaveLength(36);
        expect(resolvedValue).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i);
    });
});

