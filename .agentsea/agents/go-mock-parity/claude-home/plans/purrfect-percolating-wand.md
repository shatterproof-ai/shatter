# str-9pjd: README install metadata cleanup

## Context
README.md contains placeholder GitHub URLs (`github.com/user/shatter`) and a `TBD` license section. No LICENSE file exists. These reduce trust and onboarding clarity.

## Changes (README.md only)

### 1. Installation section (lines 5-26)
Replace the three placeholder URLs with honest "not yet published" notes:
- Remove the `curl | bash` quick install block (references nonexistent `install.sh`)
- Remove the `cargo install --git` block (references nonexistent repo)
- Keep "Build from source" but replace `git clone` URL with a note that the repo is not yet publicly available
- Rewrite Installation to show build-from-source as the primary method

### 2. License section (line 285-287)
Replace `TBD` with `Not yet determined.`

## Files
- `README.md` — the only file modified

## Verification
- Read the updated README and confirm no placeholder URLs remain
- Confirm license section is honest
- No code changes, no tests needed
