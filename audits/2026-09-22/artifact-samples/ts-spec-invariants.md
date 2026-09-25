# Shatter Explore

## `classifyNumber` *(ts/01-arithmetic.ts:10-21)*

**4 path(s)** · **100%** coverage (7/7 lines)

| # | Call | Outcome |
|---|---|---|
| 1 | `classifyNumber(0)` | returns `"zero"` |
| 2 | `classifyNumber(-1)` | returns `"negative"` |
| 3 | `classifyNumber(2)` | returns `"positive-even"` |
| 4 | `classifyNumber(1)` | returns `"positive-odd"` |

# Specification: `classifyNumber`

**Location:** `ts/01-arithmetic.ts:10-21`

**Behavioral classes:** 4  
**Exploration:** 100 iterations, 7/7 lines covered (100%)

**Function invariants:**
-  != null [1] (100/100)
-  != null [1] (100/100)
-  is non-empty [1] (100/100)

---

## Class 1 — returns "zero"

**Preconditions** [observed]:
- param[0] == 0

**Postcondition** [observed]: returns "zero"

**Invariants:**
-  != null [1] (33/33)
-  == 0 [1] (33/33)
-  >= 0 [1] (33/33)
-  != null [1] (33/33)
-  is non-empty [1] (33/33)

**Example** (33 execution(s) observed):
```
classifyNumber(0) -> "zero"
```

---

## Class 2 — returns "negative"

**Preconditions** [observed]:
- param[0] == -1

**Postcondition** [observed]: returns "negative"

**Invariants:**
-  != null [1] (33/33)
-  < 0 [1] (33/33)
-  == -1 [1] (33/33)
-  != null [1] (33/33)
-  is non-empty [1] (33/33)

**Example** (33 execution(s) observed):
```
classifyNumber(-1) -> "negative"
```

---

## Class 3 — returns "positive-even"

**Preconditions** [observed]:
- param[0] == 2

**Postcondition** [observed]: returns "positive-even"

**Invariants:**
-  != null [1] (11/11)
-  == 2 [1] (11/11)
-  > 0 [1] (11/11)
-  != null [1] (11/11)
-  is non-empty [1] (11/11)

**Example** (11 execution(s) observed):
```
classifyNumber(2) -> "positive-even"
```

---

## Class 4 — returns "positive-odd"

**Preconditions** [observed]:
- param[0] == 1

**Postcondition** [observed]: returns "positive-odd"

**Invariants:**
-  != null [1] (23/23)
-  == 1 [1] (23/23)
-  > 0 [1] (23/23)
-  != null [1] (23/23)
-  is non-empty [1] (23/23)

**Example** (23 execution(s) observed):
```
classifyNumber(1) -> "positive-odd"
```

---

**Summary:** 4 path(s) across 1 function(s) · **100%** coverage (7/7 lines)
