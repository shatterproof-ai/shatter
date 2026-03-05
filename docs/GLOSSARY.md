# Glossary

Terms used throughout the Shatter codebase and documentation.

## Exploration Strategies

**Vector strategy (vector-level strategy)**
A strategy that operates on the full input vector, deciding which combination of parameter values to try. Examples: crossover, parameter drilling, pool seeding.

**Value strategy (value-level strategy)**
A strategy that operates on a single value within the vector, deciding how to generate or mutate that individual value. Examples: char mutation, havoc, fragment injection, byte-level mutation.

**Generational search**
SAGE's technique: fix a prefix of path constraints, negate the last, solve for new inputs. Operates at the constraint level — all parameters may vary. Contrast with parameter pinning, which operates at the input-vector level where one parameter varies.

**Symbolic triage**
Using accumulated path constraints to statically predict which branch path a candidate input would take, rejecting it without execution if the predicted path is already covered. Avoids wasting execution budget on inputs that are overwhelmingly likely to be redundant. See issue str-xmtw.

## Frontier Exploration

**Blocking parameter**
The parameter in a multi-parameter input vector whose current value prevents progress past a known branch. Identified via branch-parameter attribution.

**Frontier branch**
The deepest unsatisfied branch in a path — the point in control flow where exploration is stuck. A function may have multiple frontier branches simultaneously.

**Frontier set**
The collection of all frontier branches for a function, each with its blocking parameters, best-known prefix, and stall count. Explored via priority-based selection.

**Branch-parameter attribution**
Determining which parameter positions appear in a branch's symbolic constraint. Extracted by walking the `SymExpr` tree for `Param` nodes.

**Parameter pinning**
Fixing non-blocking parameters at known-good values while varying only the blocking parameter. The input-vector-level analog of SAGE's constraint-level generational search.

**Prefix pinning**
Synonym for parameter pinning when the "fixed" parameters are conceptualized as a prefix to the varying ones.

**Parameter drilling**
Intensive mutation/generation of a single blocking parameter while other parameters are pinned. A vector-level strategy that delegates to aggressive value-level tactics on the blocking parameter.

## Nondeterminism

**Nondeterminism evidence**
The basis for flagging a return value field as nondeterministic. The orientation is asymmetric: we can prove nondeterminism (observed different values from identical inputs) but can never prove determinism — only "not yet observed to vary." Absence from the nondeterminism list does NOT assert determinism.

**Structural similarity**
A comparison metric for JSON return values based on the fraction of matching leaf values. Used to distinguish minor nondeterministic drift (one field changed out of many) from genuine behavioral regressions (most fields changed).

## Farming

**Farming**
Running long exploration sessions (hours/overnight) with high iteration limits to discover interesting inputs, saved permanently for cross-run reuse via the seed pool.

**Consolidator**
A process that merges outputs from multiple independent farming workers into the canonical seed pool, deduplicating entries and applying the eviction policy.
