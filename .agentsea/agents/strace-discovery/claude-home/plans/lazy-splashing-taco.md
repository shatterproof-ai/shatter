# Plan: Risk Map and Diff Classifier (flt-it8.32.4)

## Context

The governance hardening plan (Phase 2) calls for `policy/risk-map.json` and `scripts/ci/detect-risk.py`. Both already exist as untracked files in the main repo with solid implementations. This task brings them into the worktree branch, adds missing features per the issue spec (risk severity levels, exit codes for CI gating), adds tests, and documents the risk model.

## What Exists (in main repo, untracked)

- **`policy/risk-map.json`**: 7 risk classes with globs, content patterns, and recommended checks. Missing: severity/risk levels (critical/high/medium/low) and required-checks distinction.
- **`scripts/ci/detect-risk.py`**: Functional diff classifier. Missing: exit code differentiation (0 for low, 1 for high/critical), severity-based output, `--help` documentation.

## Deliverables

### 1. Enhance `policy/risk-map.json`
- Add `"severity"` field to each class: `critical`, `high`, `medium`, `low`
- Severity mapping:
  - `trust_boundary` → critical
  - `client_secret_storage` → high
  - `public_contract` → medium
  - `go_surface` → low
  - `web_surface` → low
  - `android_surface` → low
  - `extension_surface` → low
- Add `"required_checks"` (mandatory) vs keeping `"recommended_checks"` (advisory)
- Add top-level documentation comment field explaining the risk model

### 2. Enhance `scripts/ci/detect-risk.py`
- Add exit code logic: return 1 if any matched class has severity `critical` or `high`
- Add `--help` text with examples
- Print severity in human-readable output
- Include severity in JSON output
- Add `--severity-threshold` flag (default: `high`) — exit 1 if any class meets or exceeds threshold

### 3. Add tests: `scripts/ci/test_detect_risk.py`
- Test glob matching logic
- Test content pattern matching
- Test severity threshold / exit code behavior
- Test JSON output format
- Test `--all` / `--staged` / `--base` mode selection
- Use unittest with mocked git commands (no real repo needed)

### 4. Documentation
- Add docstring/header comments in both files explaining the risk model
- Risk classes documented inline in risk-map.json descriptions

## Files to Create/Modify

| File | Action |
|------|--------|
| `policy/risk-map.json` | Create (copy from main + enhance) |
| `scripts/ci/detect-risk.py` | Create (copy from main + enhance) |
| `scripts/ci/test_detect_risk.py` | Create new |

## Verification

```bash
# Help text works
python3 scripts/ci/detect-risk.py --help

# Classify entire repo
python3 scripts/ci/detect-risk.py --all

# JSON output
python3 scripts/ci/detect-risk.py --all --json

# Run tests
python3 -m pytest scripts/ci/test_detect_risk.py -v

# Exit code test — should return 1 for repo with trust_boundary files
python3 scripts/ci/detect-risk.py --all; echo "Exit: $?"
```
