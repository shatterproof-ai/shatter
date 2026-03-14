# Post-provision manual setup script

## Context

After Ansible provisioning, the user must manually run `netbird up` (browser auth) and will now also want to authenticate with GitHub CLI and clone recently-active repos. These are interactive steps that can't be automated in Ansible. A single script in `~/setup/` will handle all post-provision manual steps.

## Changes

### 1. Install `gh` CLI — new Ansible role `roles/github-cli/`

`roles/github-cli/tasks/main.yml`:
- Add the GitHub CLI apt repository and install `gh`
- Standard pattern matching other roles (e.g., netbird)

Add `github-cli` to `playbook.yml` (before `user` role).

### 2. Create `~/setup/post-provision.sh` via the `user` role

Add a task to `roles/user/tasks/main.yml` that templates a script to `~/setup/post-provision.sh`. The script will:

1. **`gh auth login`** — device code flow (interactive, opens browser)
2. **Clone recent repos** — uses `gh api` to search commits by the user's configured git email in the last 60 days, extracts unique repo SSH URLs, skips any already cloned in `~/project/`
3. **`netbird up`** — interactive browser auth for mesh VPN

The git email is already available as an Ansible variable (`git_email`), so it can be templated into the script.

### 3. Update provisioning output

Update the "manual steps" message in `wsl-dev.ps1` to reference `~/setup/post-provision.sh` instead of listing `netbird up` alone.

## Files to modify

- `roles/github-cli/tasks/main.yml` — **new file**
- `playbook.yml` — add `github-cli` role
- `roles/user/tasks/main.yml` — add task to create `~/setup/post-provision.sh`
- `wsl-dev.ps1` — update post-provision instructions (line ~466)

## Verification

- Run `ansible-playbook playbook.yml --diff` on an existing instance
- Confirm `gh` is installed
- Confirm `~/setup/post-provision.sh` exists with correct content
- Run `~/setup/post-provision.sh` and verify it prompts for gh auth, clones repos, and runs netbird up
