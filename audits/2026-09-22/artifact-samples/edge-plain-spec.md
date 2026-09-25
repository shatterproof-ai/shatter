# Shatter Explore

## `fmt2` *(edge/plain.ts:1-9)*

**0 path(s)** · **100%** coverage (5/5 lines)


# Specification: `fmt2`

**Location:** `edge/plain.ts:1-9`

**Behavioral classes:** 3  
**Exploration:** 30 iterations, 5/5 lines covered (100%)

---

## Class 1 — returns "big"

**Preconditions** [observed]:
- param[0] > 0

**Postcondition** [observed]: returns "big"

**Example** (15 execution(s) observed):
```
fmt2(11) -> "big"
```

---

## Class 2 — returns "neg"

**Preconditions** [observed]:
- param[0] < 0

**Postcondition** [observed]: returns "neg"

**Example** (8 execution(s) observed):
```
fmt2(-1) -> "neg"
```

---

## Class 3 — throws Error: bad

**Preconditions** [observed]:
- typeof param[0] == "number"

**Postcondition** [observed]: throws Error: bad

**Example** (12 execution(s) observed):
```
fmt2(10) throws Error: bad
```

---

**Summary:** 0 path(s) across 1 function(s) · **100%** coverage (5/5 lines)
