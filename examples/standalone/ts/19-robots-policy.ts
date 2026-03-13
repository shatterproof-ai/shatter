// Example 19: robots.txt rule evaluation
// Adapted from samclarke/robots-parser (MIT): https://github.com/samclarke/robots-parser
// Exercises directive parsing, user-agent grouping, wildcard path matching,
// end-anchor handling, and longest-rule precedence.
//
// EXPECTED BRANCHES for evaluateRobotsPolicy (15):
//   1. empty user agent                          -> missing-user-agent
//   2. path missing leading slash                -> invalid-path
//   3. comments and blank lines ignored          -> parse continues
//   4. unknown directives ignored                -> parse continues
//   5. specific user-agent group beats wildcard  -> specific group selected
//   6. wildcard user-agent fallback              -> wildcard group selected
//   7. no matching group                         -> allow by default
//   8. empty disallow directive                  -> ignored
//   9. valid crawl-delay parsed                  -> crawlDelay populated
//  10. invalid crawl-delay ignored               -> crawlDelay absent
//  11. wildcard '*' in rule matches segment      -> rule applies
//  12. '$' end-anchor enforced                   -> suffix-sensitive match
//  13. longer allow beats shorter disallow       -> allowed
//  14. longer disallow beats allow               -> disallowed
//  15. equal-length tie resolved in favor allow  -> allowed
//
// DIFFICULTY: Hard. The solver must synthesize structured multi-line rule sets
// with interacting directives and carefully chosen paths.

interface RobotsRule {
    directive: "allow" | "disallow";
    pattern: string;
}

interface RobotsGroup {
    agents: string[];
    rules: RobotsRule[];
    crawlDelay: number | null;
}

interface RobotsDecision {
    allowed: boolean;
    reason: string;
    matchedDirective: string | null;
    crawlDelay: number | null;
}

function normalizeAgent(agent: string): string {
    const lower = agent.trim().toLowerCase();
    const slash = lower.indexOf("/");
    return slash >= 0 ? lower.slice(0, slash) : lower;
}

function escapeRegex(pattern: string): string {
    return pattern.replace(/[|\\{}()[\]^$+?.]/g, "\\$&");
}

function matchesPattern(pattern: string, path: string): boolean {
    const anchored = pattern.length > 0 && pattern.charAt(pattern.length - 1) === "$";
    const body = anchored ? pattern.slice(0, -1) : pattern;
    const regexBody = escapeRegex(body).replace(/\*/g, ".*");
    const source = anchored ? `^${regexBody}$` : `^${regexBody}`;
    return new RegExp(source).test(path);
}

function ruleSpecificity(pattern: string): number {
    return pattern.replace(/\*/g, "").replace(/\$/g, "").length;
}

function parseRobots(robotsTxt: string): RobotsGroup[] {
    const groups: RobotsGroup[] = [];
    let current: RobotsGroup | null = null;

    for (const rawLine of robotsTxt.replace(/\r\n?/g, "\n").split("\n")) {
        const withoutComment = rawLine.split("#", 1)[0].trim();
        if (withoutComment.length === 0) {
            continue;
        }

        const colon = withoutComment.indexOf(":");
        if (colon < 0) {
            continue;
        }

        const key = withoutComment.slice(0, colon).trim().toLowerCase();
        const value = withoutComment.slice(colon + 1).trim();

        if (key === "user-agent") {
            if (current === null || current.rules.length > 0 || current.crawlDelay !== null) {
                current = { agents: [], rules: [], crawlDelay: null };
                groups.push(current);
            }
            current.agents.push(normalizeAgent(value));
            continue;
        }

        if (current === null) {
            continue;
        }

        if (key === "allow" || key === "disallow") {
            if (key === "disallow" && value.length === 0) {
                continue;
            }
            current.rules.push({ directive: key, pattern: value });
            continue;
        }

        if (key === "crawl-delay") {
            const parsed = Number(value);
            if (isFinite(parsed) && parsed >= 0) {
                current.crawlDelay = parsed;
            }
        }
    }

    return groups;
}

function groupMatches(agent: string, group: RobotsGroup): boolean {
    for (const candidate of group.agents) {
        if (candidate === "*") {
            continue;
        }
        if (agent.slice(0, candidate.length) === candidate) {
            return true;
        }
    }
    return false;
}

export function evaluateRobotsPolicy(
    robotsTxt: string,
    userAgent: string,
    path: string
): RobotsDecision {
    const normalizedAgent = normalizeAgent(userAgent);
    if (normalizedAgent.length === 0) {
        return {
            allowed: false,
            reason: "missing-user-agent",
            matchedDirective: null,
            crawlDelay: null,
        };
    }
    if (path.charAt(0) !== "/") {
        return {
            allowed: false,
            reason: "invalid-path",
            matchedDirective: null,
            crawlDelay: null,
        };
    }

    const groups = parseRobots(robotsTxt);
    const specificGroups = groups.filter(group => groupMatches(normalizedAgent, group));
    const wildcardGroups = groups.filter(group => group.agents.indexOf("*") !== -1);
    const selectedGroups = specificGroups.length > 0 ? specificGroups : wildcardGroups;

    if (selectedGroups.length === 0) {
        return {
            allowed: true,
            reason: "no-group",
            matchedDirective: null,
            crawlDelay: null,
        };
    }

    let crawlDelay: number | null = null;
    let bestRule: RobotsRule | null = null;
    let bestSpecificity = -1;

    for (const group of selectedGroups) {
        if (crawlDelay === null && group.crawlDelay !== null) {
            crawlDelay = group.crawlDelay;
        }

        for (const rule of group.rules) {
            if (!matchesPattern(rule.pattern, path)) {
                continue;
            }

            const specificity = ruleSpecificity(rule.pattern);
            const betterDirective =
                bestRule !== null &&
                specificity === bestSpecificity &&
                rule.directive === "allow" &&
                bestRule.directive === "disallow";

            if (specificity > bestSpecificity || bestRule === null || betterDirective) {
                bestRule = rule;
                bestSpecificity = specificity;
            }
        }
    }

    if (bestRule === null) {
        return {
            allowed: true,
            reason: "no-rule",
            matchedDirective: null,
            crawlDelay,
        };
    }

    return {
        allowed: bestRule.directive === "allow",
        reason: bestRule.directive,
        matchedDirective: `${bestRule.directive}:${bestRule.pattern}`,
        crawlDelay,
    };
}
