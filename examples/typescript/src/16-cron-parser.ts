// Example 16: Cron expression parser
// Parses cron expressions into structured schedule descriptors.
// Exercises field-level validation, range/step/list parsing, and special
// string shortcuts — a compact format with surprisingly deep branching.
//
// EXPECTED BRANCHES for parseCron (22):
//   1. empty string                             → throws Error("empty expression")
//   2. special "@yearly" or "@annually"         → { minute:0, hour:0, day:1, month:1, weekday:* }
//   3. special "@monthly"                       → { minute:0, hour:0, day:1, month:*, weekday:* }
//   4. special "@weekly"                        → { minute:0, hour:0, day:*, month:*, weekday:0 }
//   5. special "@daily" or "@midnight"          → { minute:0, hour:0, day:*, month:*, weekday:* }
//   6. special "@hourly"                        → { minute:0, hour:*, day:*, month:*, weekday:* }
//   7. unknown special string                   → throws Error("unknown special")
//   8. wrong number of fields (not 5)           → throws Error("expected 5 fields")
//   9. wildcard "*" in field                    → full range for that field
//  10. single number in range                   → exact value
//  11. number below field minimum               → throws Error("value below minimum")
//  12. number above field maximum               → throws Error("value above maximum")
//  13. range "a-b"                              → values from a to b inclusive
//  14. range with start > end                   → throws Error("invalid range")
//  15. step "*/n"                               → every nth value from min
//  16. step "a/n"                               → every nth value from a
//  17. step value zero                          → throws Error("step cannot be zero")
//  18. list "a,b,c"                             → union of values
//  19. list element invalid                     → throws Error("invalid value in list")
//  20. non-numeric value                        → throws Error("non-numeric value")
//  21. range within list "1,3-5,7"              → union includes range expansion
//  22. weekday field with 7 treated as 0 (Sunday) → normalized
//
// DIFFICULTY: Hard. Each of the 5 fields can independently be a wildcard,
// number, range, step, or list — combinatorial explosion of valid structures.
// The solver must generate syntactically precise strings with correct delimiters.

interface CronField {
    values: number[];
}

interface CronSchedule {
    minute: CronField;
    hour: CronField;
    dayOfMonth: CronField;
    month: CronField;
    weekday: CronField;
}

const FIELD_BOUNDS: [number, number][] = [
    [0, 59],   // minute
    [0, 23],   // hour
    [1, 31],   // day of month
    [1, 12],   // month
    [0, 6],    // weekday (0=Sunday)
];

function range(start: number, end: number): number[] {
    const result: number[] = [];
    for (let i = start; i <= end; i++) {
        result.push(i);
    }
    return result;
}

function parseField(field: string, min: number, max: number): CronField {
    // Wildcard
    if (field === "*") {
        return { values: range(min, max) };
    }

    // Step: */n or a/n
    if (field.includes("/")) {
        const [baseStr, stepStr] = field.split("/");
        const step = parseInt(stepStr, 10);
        if (isNaN(step)) {
            throw new Error("non-numeric value");
        }
        if (step === 0) {
            throw new Error("step cannot be zero");
        }
        const start = baseStr === "*" ? min : parseInt(baseStr, 10);
        if (isNaN(start)) {
            throw new Error("non-numeric value");
        }
        const values: number[] = [];
        for (let i = start; i <= max; i += step) {
            values.push(i);
        }
        return { values };
    }

    // List: a,b,c (may contain ranges like 1,3-5,7)
    if (field.includes(",")) {
        const values: number[] = [];
        for (const part of field.split(",")) {
            if (part.includes("-")) {
                const parsed = parseRange(part, min, max);
                values.push(...parsed);
            } else {
                const val = parseInt(part, 10);
                if (isNaN(val)) {
                    throw new Error("invalid value in list");
                }
                if (val < min) throw new Error("value below minimum");
                if (val > max) throw new Error("value above maximum");
                values.push(val);
            }
        }
        return { values };
    }

    // Range: a-b
    if (field.includes("-")) {
        return { values: parseRange(field, min, max) };
    }

    // Single number
    const val = parseInt(field, 10);
    if (isNaN(val)) {
        throw new Error("non-numeric value");
    }
    if (val < min) throw new Error("value below minimum");
    if (val > max) throw new Error("value above maximum");
    return { values: [val] };
}

function parseRange(expr: string, min: number, max: number): number[] {
    const [startStr, endStr] = expr.split("-");
    const start = parseInt(startStr, 10);
    const end = parseInt(endStr, 10);
    if (isNaN(start) || isNaN(end)) {
        throw new Error("non-numeric value");
    }
    if (start > end) {
        throw new Error("invalid range");
    }
    if (start < min) throw new Error("value below minimum");
    if (end > max) throw new Error("value above maximum");
    return range(start, end);
}

function allValues(min: number, max: number): CronField {
    return { values: range(min, max) };
}

export function parseCron(expression: string): CronSchedule {
    if (expression.length === 0) {
        throw new Error("empty expression");
    }

    // Special strings
    if (expression.startsWith("@")) {
        switch (expression.toLowerCase()) {
            case "@yearly":
            case "@annually":
                return {
                    minute: { values: [0] }, hour: { values: [0] },
                    dayOfMonth: { values: [1] }, month: { values: [1] },
                    weekday: allValues(0, 6),
                };
            case "@monthly":
                return {
                    minute: { values: [0] }, hour: { values: [0] },
                    dayOfMonth: { values: [1] }, month: allValues(1, 12),
                    weekday: allValues(0, 6),
                };
            case "@weekly":
                return {
                    minute: { values: [0] }, hour: { values: [0] },
                    dayOfMonth: allValues(1, 31), month: allValues(1, 12),
                    weekday: { values: [0] },
                };
            case "@daily":
            case "@midnight":
                return {
                    minute: { values: [0] }, hour: { values: [0] },
                    dayOfMonth: allValues(1, 31), month: allValues(1, 12),
                    weekday: allValues(0, 6),
                };
            case "@hourly":
                return {
                    minute: { values: [0] }, hour: allValues(0, 23),
                    dayOfMonth: allValues(1, 31), month: allValues(1, 12),
                    weekday: allValues(0, 6),
                };
            default:
                throw new Error("unknown special");
        }
    }

    const fields = expression.trim().split(/\s+/);
    if (fields.length !== 5) {
        throw new Error("expected 5 fields");
    }

    const schedule: CronSchedule = {
        minute: parseField(fields[0], FIELD_BOUNDS[0][0], FIELD_BOUNDS[0][1]),
        hour: parseField(fields[1], FIELD_BOUNDS[1][0], FIELD_BOUNDS[1][1]),
        dayOfMonth: parseField(fields[2], FIELD_BOUNDS[2][0], FIELD_BOUNDS[2][1]),
        month: parseField(fields[3], FIELD_BOUNDS[3][0], FIELD_BOUNDS[3][1]),
        weekday: parseField(fields[4], FIELD_BOUNDS[4][0], FIELD_BOUNDS[4][1]),
    };

    // Normalize weekday 7 → 0 (Sunday)
    schedule.weekday.values = schedule.weekday.values.map(v => v === 7 ? 0 : v);

    return schedule;
}
