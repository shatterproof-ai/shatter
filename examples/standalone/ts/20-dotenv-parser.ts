// Example 20: dotenv parser
// Adapted from motdotla/dotenv (BSD-2-Clause): https://github.com/motdotla/dotenv
// Exercises line parsing, export-prefix handling, quoted values, comment
// stripping, multiline quoted values, and invalid-line recovery.
//
// EXPECTED BRANCHES for parseDotenv (16):
//   1. empty input                               -> empty result
//   2. blank lines ignored                       -> skipped
//   3. comment lines ignored                     -> skipped
//   4. invalid line without separator            -> warning emitted
//   5. invalid key characters                    -> warning emitted
//   6. export prefix stripped                    -> key parsed
//   7. '=' separator parsed                      -> value assigned
//   8. ':' separator parsed                      -> value assigned
//   9. empty value allowed                       -> empty string
//  10. unquoted values trimmed                   -> whitespace removed
//  11. inline comment on unquoted value removed  -> comment stripped
//  12. single-quoted value preserved             -> escapes left literal
//  13. double-quoted value expands \n and \r     -> escaped newlines expanded
//  14. hash inside quoted value preserved        -> not treated as comment
//  15. multiline quoted value consumed           -> joins following lines
//  16. unterminated quoted value                 -> warning emitted
//
// DIFFICULTY: Hard. The solver must craft line-oriented text with quotes,
// separators, comments, and multiline structure that interact precisely.

interface DotenvResult {
    values: Record<string, string>;
    warnings: string[];
}

function findSeparator(line: string): number {
    const eq = line.indexOf("=");
    const colon = line.indexOf(":");
    if (eq === -1) return colon;
    if (colon === -1) return eq;
    return Math.min(eq, colon);
}

function stripInlineComment(value: string): string {
    let inSingle = false;
    let inDouble = false;
    let inBacktick = false;

    for (let i = 0; i < value.length; i++) {
        const ch = value[i];
        if (ch === "'" && !inDouble && !inBacktick) {
            inSingle = !inSingle;
        } else if (ch === "\"" && !inSingle && !inBacktick) {
            inDouble = !inDouble;
        } else if (ch === "`" && !inSingle && !inDouble) {
            inBacktick = !inBacktick;
        } else if (ch === "#" && !inSingle && !inDouble && !inBacktick) {
            return value.slice(0, i);
        }
    }

    return value;
}

export function parseDotenv(src: string): DotenvResult {
    const values: Record<string, string> = {};
    const warnings: string[] = [];
    const lines = src.replace(/\r\n?/g, "\n").split("\n");

    for (let i = 0; i < lines.length; i++) {
        let line = lines[i].trim();
        if (line.length === 0 || line.charAt(0) === "#") {
            continue;
        }

        if (line.slice(0, "export ".length) === "export ") {
            line = line.slice("export ".length).trim();
        }

        const sep = findSeparator(line);
        if (sep < 0) {
            warnings.push(`line ${i + 1}: missing separator`);
            continue;
        }

        const key = line.slice(0, sep).trim();
        if (!/^[A-Za-z0-9_.-]+$/.test(key)) {
            warnings.push(`line ${i + 1}: invalid key`);
            continue;
        }

        let rawValue = line.slice(sep + 1).replace(/^\s+/, "");
        if (rawValue.length === 0) {
            values[key] = "";
            continue;
        }

        const quote = rawValue[0];
        if (quote === "'" || quote === "\"" || quote === "`") {
            let body = rawValue.slice(1);
            let closed = false;

            while (true) {
                const quoteIndex = body.indexOf(quote);
                if (quoteIndex >= 0) {
                    body = body.slice(0, quoteIndex);
                    closed = true;
                    break;
                }
                if (i + 1 >= lines.length) {
                    break;
                }
                i += 1;
                body += "\n" + lines[i];
            }

            if (!closed) {
                warnings.push(`line ${i + 1}: unterminated quote`);
                continue;
            }

            if (quote === "\"") {
                body = body.replace(/\\n/g, "\n").replace(/\\r/g, "\r");
            }

            values[key] = body;
            continue;
        }

        rawValue = stripInlineComment(rawValue).trim();
        values[key] = rawValue;
    }

    return { values, warnings };
}
