## Standalone Example Attribution

The mirrored standalone examples `18`, `19`, and `20` are adapted from
permissively licensed GitHub projects:

- `18-accept-language`
  Source: `jshttp/negotiator`
  URL: https://github.com/jshttp/negotiator
  License: MIT
- `19-robots-policy`
  Source: `samclarke/robots-parser`
  URL: https://github.com/samclarke/robots-parser
  License: MIT
- `20-dotenv-parser`
  Source: `motdotla/dotenv`
  URL: https://github.com/motdotla/dotenv
  License: BSD-2-Clause

These files are not verbatim copies. They are deliberately reduced,
cross-language adaptations shaped to match Shatter's standalone example style:
single-file, self-contained logic with branch-dense behavior and stable parity
across TypeScript, Go, and Rust.

## Walkthrough Selection

Only `18-accept-language` is included in the guided walkthrough command sets.
It is the best live-demo target of the new examples because it is:

- pure and deterministic
- branch-dense without requiring long multiline inputs
- easy to explain from terminal output

`19-robots-policy` and `20-dotenv-parser` remain valuable for scan and
regression coverage, but they are better suited to broader testing than to the
interactive walkthrough.
