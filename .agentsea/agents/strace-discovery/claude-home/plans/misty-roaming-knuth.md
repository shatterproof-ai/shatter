# Contract Registry Validator — kapow-n92.3

## Context
The contracts/ directory has JSON Schemas for product and technical contracts, plus examples, but no validation tooling. We need a script that validates contract registry files against schemas, checks field values, verifies file references, and enforces shipped-claim linkage rules.

## Implementation

### 1. Create `scripts/validate-contracts.sh`

Use **python3 + jsonschema** (already installed, v4.10.3) for JSON Schema validation, with jq for supplementary checks.

**Discovery**: Find all `.json` files in `contracts/` excluding `schema/` and `examples/` dirs.

**Validation pipeline per file**:
1. **Detect contract type**: Check if file contains `"guarantees"` or `"category"` with technical values → technical schema; otherwise → product schema
2. **JSON Schema validation**: `python3 -c "import json, jsonschema; ..."` against the appropriate schema
3. **Enum value checks** (belt-and-suspenders, schema covers these too):
   - `status`: `shipped|experimental|planned|deprecated`
   - `category`: product (`feature|page|integration|data-quality`) or technical (`api|search|auth|data|performance|infrastructure`)
   - `evidence.type`: `test|manual|monitoring|assertion`
4. **Source file existence**: Warn (not fail) if `source_files[]` paths don't exist in repo
5. **Test file existence**: Warn (not fail) if `tests[]` paths don't exist in repo
6. **Shipped linkage rule**: If `status == "shipped"`, require at least one entry in `tests[]` OR one entry in `issues[]`
7. **Output**: Clear pass/fail per file, actionable error messages, exit 1 on any failure

**Contract type detection logic**: Use a simple heuristic — if the file has a `"guarantees"` key, validate against technical schema; otherwise use product schema. Also support arrays of contracts (the registry files will likely be arrays).

### 2. Create `scripts/validate-contracts_test.sh`

Follow the pattern from `scripts/classify-changes_test.sh` (PASS/FAIL counters, colored output).

Test cases using temp files:
- Valid product contract → PASS
- Valid technical contract → PASS
- Missing required field (no `title`) → FAIL
- Bad enum value (`status: "bogus"`) → FAIL
- Shipped contract with no tests and no issues → FAIL
- Shipped contract with tests → PASS
- Shipped contract with issues only → PASS
- Malformed JSON → FAIL
- Source file that doesn't exist → WARN (but still passes)

### 3. Add Makefile target

```makefile
validate-contracts:
	./scripts/validate-contracts.sh
```

## Files to create/modify
- `scripts/validate-contracts.sh` (new)
- `scripts/validate-contracts_test.sh` (new)
- `Makefile` (add target)

## Verification
```bash
# Run the test suite
./scripts/validate-contracts_test.sh

# Run against actual contract files (if any exist)
make validate-contracts

# Existing tests still pass
make test-quick
```
