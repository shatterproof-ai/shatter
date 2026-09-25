# Shatter Explore

## `classifyNumber` *(arithmetic-v1.ts:11-22)*

**4 path(s)** · **100%** coverage (7/7 lines)

| # | Call | Outcome |
|---|---|---|
| 1 | `classifyNumber(0)` | returns `"zero"` |
| 2 | `classifyNumber(-1)` | returns `"negative"` |
| 3 | `classifyNumber(2)` | returns `"positive-even"` |
| 4 | `classifyNumber(1)` | returns `"positive-odd"` |

# Specification: `classifyNumber`

**Location:** `arithmetic-v1.ts:11-22`

**Behavioral classes:** 4  
**Exploration:** 20 iterations, 7/7 lines covered (100%)

---

## Class 1 — returns "zero"

**Preconditions** [observed]:
- param[0] == 0

**Postcondition** [observed]: returns "zero"

**Example** (7 execution(s) observed):
```
classifyNumber(0) -> "zero"
```

---

## Class 2 — returns "negative"

**Preconditions** [observed]:
- param[0] == -1

**Postcondition** [observed]: returns "negative"

**Example** (6 execution(s) observed):
```
classifyNumber(-1) -> "negative"
```

---

## Class 3 — returns "positive-even"

**Preconditions** [observed]:
- param[0] == 2

**Postcondition** [observed]: returns "positive-even"

**Example** (3 execution(s) observed):
```
classifyNumber(2) -> "positive-even"
```

---

## Class 4 — returns "positive-odd"

**Preconditions** [observed]:
- param[0] == 1

**Postcondition** [observed]: returns "positive-odd"

**Example** (4 execution(s) observed):
```
classifyNumber(1) -> "positive-odd"
```

---

**Summary:** 4 path(s) across 1 function(s) · **100%** coverage (7/7 lines)
