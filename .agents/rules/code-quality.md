# Code Quality Standards

## Zero Defect Mindset

All code is production code. Write every line as if it will handle real users, real money, and real data under adversarial conditions. No prototypes, no "fix later" comments, no shortcuts. If it ships, it must be correct, secure, and maintainable.

## Inline Documentation

Comments explain **why** or document **non-obvious contracts** — never restate what the code already says. If a name needs a comment, choose a better name first.

**What to document:**
- Public API contracts: preconditions, postconditions, error behavior, ownership semantics
- Non-obvious design choices: why an algorithm was chosen, why a field exists, why ordering matters
- Lint/warning suppressions: always explain why the suppression is needed
- Security-sensitive decisions: why a particular validation exists, why access is restricted

**What NOT to document:**
- What the code does when the code already says it (`// returns the sum` above `fn sum()`)
- Type information visible in the signature (`// takes a string` above `fn foo(s: string)`)
- Existence of language constructs (`// uses a switch statement`)

**The delete test:** If you can delete a comment and the code is equally clear, delete it.

## No Unused Code

Dead code — unused imports, unreachable branches, commented-out blocks, vestigial functions — must never be checked in. It obscures intent, triggers compiler/linter warnings, and rots over time. Temporarily commenting out code for local debugging is acceptable, but it must be removed or restored before commit.

If the compiler or linter warns about unused code, the correct response is one of:
1. **Delete it** — if the code is genuinely no longer needed.
2. **Use it as intended** — if it was left unused due to an oversight or incomplete implementation.

"Keeping it around in case we need it later" is not a valid reason — that's what version control is for.

### Never Suppress Unused-Code Warnings

Lint suppression directives must **never** be used to silence unused-code warnings. These directives hide the problem instead of fixing it:

- **Rust**: `#[allow(dead_code)]`, `#[allow(unused_imports)]`, `#[allow(unused_variables)]`, `#[allow(unused_mut)]`, or any `#[allow(unused_*)]`
- **TypeScript/JavaScript**: `// eslint-disable-next-line @typescript-eslint/no-unused-vars`, `// @ts-ignore` or `// @ts-expect-error` to hide unused bindings
- **Go**: `_` assignments solely to satisfy the compiler (e.g., `_ = unusedVar`) when the variable itself should be removed

Prefixing a variable with `_` (in Rust or Go) is acceptable **only** when the binding is structurally required — e.g., a destructured field you genuinely don't need, or a function parameter imposed by a trait/interface contract. It is not acceptable as a way to keep dead code around.

The same principle applies to all other suppression mechanisms across languages. If a warning says code is unused, fix the root cause.

## Issue Scope Discipline

Only modify files that are directly required by the assigned issue. Do not make "while I'm here" changes to unrelated files, even if the change is correct and useful.

Before committing, review every changed file and ask: "is this change required by my issue?" If not, revert it. Incidental improvements belong in separate issues — create one and move on.

## No Magic Numbers or String Literals

Define named constants for default values, timeouts, error codes, capability lists, and any value that appears in both production code and tests. Tests must reference the constant, not duplicate the literal.

## No Hardcoded Absolute Paths

Never write a literal absolute path (e.g., `"/home/user/project/..."`) in code, tests, output artifacts, or documentation. Runtime-computed absolute paths are fine — use the language's path resolution utilities. The rule prohibits hardcoded paths that bake in machine-specific locations.

## Parallel Code Paths Must Maintain Parity

When two functions process the same domain, they must handle the same cases. When adding a capability to one path, check the other. Grep for the parallel code path before declaring done.

## Security

Security is not a feature — it is a constraint on all features. Every code change must satisfy these requirements.

### Input Validation
- **Validate all external input** at system boundaries (HTTP handlers, CLI args, file parsers, message consumers). Never trust upstream data.
- **Allowlist over denylist**: define what is permitted, reject everything else.
- **Enforce size limits** on all inputs: request bodies, file uploads, string fields, array lengths. Unbounded input is a denial-of-service vector.

### Injection Prevention
- **Never use string interpolation to build SQL** — always use parameterized queries.
- **Sanitize output** for the target context (HTML, JSON, shell commands). Use framework-provided escaping, not manual string replacement.
- **Never pass unsanitized input to shell commands, eval, or template engines.**

### Authentication & Authorization
- **Check authorization on every request**, not just at the UI layer. Backend endpoints must enforce access control independently.
- **Never expose internal IDs** that allow enumeration without authorization checks.
- **Use constant-time comparison** for secrets and tokens.

### Secrets & Credentials
- **Never commit** `.env` or files containing secrets — only `.env.example` with placeholder values.
- **Never log secrets**, tokens, passwords, or PII — even at debug level.
- **Never hardcode credentials** in source code. Use environment variables or secret managers.

### Dependencies
- **Never add a dependency without checking its maintenance status** — last release date, open security advisories, download trends.
- **Pin dependency versions** in lock files. Review changelogs before upgrading.
- **Never add** `node_modules/`, `dist/`, `target/`, or build output to git.

### OWASP Awareness
When working on web-facing code, actively guard against the OWASP Top 10: injection, broken auth, sensitive data exposure, XXE, broken access control, misconfig, XSS, insecure deserialization, vulnerable components, insufficient logging. If you are unsure whether a pattern is safe, it is not safe — flag it.

### Error Handling
- **Never expose stack traces, internal paths, or implementation details** in user-facing error messages.
- **Log security-relevant failures** (auth failures, validation rejections, permission denials) with enough context for incident response, but without leaking sensitive data into logs.

## File Format Preferences

- **YAML** for structured data and configuration files
- **Markdown** for formatted text (documentation, plans, notes, specs)
