# Dropkick CI: Implementation Plan

## Context

Build a polling-based CI system for a WSL2 Linux VM that monitors GitHub repos via SSH, runs arbitrary test commands (including Dagger), and provides build history via Laminar CI's web UI. The system must be trivially rebuildable from scratch via Ansible, bootstrapped from Windows via PowerShell, and make it dead simple to enroll new projects.

## Architecture

```
Windows boots
  -> Task Scheduler runs startup.ps1
    -> Reads vault password from Windows Credential Manager
    -> Pipes to /run/dropkick/vault-pass in WSL VM

WSL VM (Ubuntu, systemd enabled)
  -> systemd timer fires every 5 min
    -> poll.sh reads /etc/dropkick/repos.yaml
    -> git ls-remote for each repo (SSH)
    -> Compares hash to /var/lib/dropkick/state/<name>/last-commit
    -> On change: laminarc run <job>
      -> Laminar runs /var/lib/laminar/cfg/jobs/<name>.run
        -> cd $WORKSPACE, fetch, checkout, run command
      -> On failure: .after script emails via msmtp
```

## File Structure

```
dropkick/
├── .gitignore
├── CLAUDE.md
├── config/
│   └── repos.yaml                          # User-facing: list of repos to poll
├── ansible/
│   ├── ansible.cfg
│   ├── inventory.yml
│   ├── playbook.yml
│   ├── group_vars/
│   │   └── all.yml                         # Default variables
│   ├── vault/
│   │   └── secrets.yml                     # Encrypted: SSH key, SMTP creds
│   └── roles/
│       ├── base/
│       │   └── tasks/main.yml              # User, SSH key, packages, Docker, Dagger
│       ├── laminar/
│       │   ├── tasks/main.yml              # Build from source + configure
│       │   ├── handlers/main.yml
│       │   └── templates/laminar.conf.j2
│       ├── ci-poller/
│       │   ├── tasks/main.yml              # Deploy scripts, generate jobs, systemd timer
│       │   ├── handlers/main.yml
│       │   ├── files/
│       │   │   ├── poll.sh                 # Core polling script
│       │   │   └── notify.sh               # Email helper
│       │   └── templates/
│       │       ├── dropkick-poll.service.j2
│       │       ├── dropkick-poll.timer.j2
│       │       ├── logrotate-dropkick.j2
│       │       ├── job.run.j2              # Per-repo Laminar job
│       │       └── job.after.j2            # Per-repo failure notification
│       ├── msmtp/
│       │   ├── tasks/main.yml
│       │   └── templates/msmtprc.j2
│       └── netbird/
│           └── tasks/main.yml
└── windows/
    ├── setup.ps1                           # Create/update WSL distro
    └── startup.ps1                         # Boot: pipe vault password to VM
```

## Implementation Steps (ordered)

### Step 1: Project scaffolding
- `.gitignore`: `*.retry`, `__pycache__/`, `.vagrant/`
- `CLAUDE.md`: Project conventions (bash: `set -euo pipefail`, Ansible standard roles, repos.yaml as source of truth)

### Step 2: config/repos.yaml
```yaml
poll_interval_minutes: 5
notification:
  email_to: "ketan@example.com"
  email_from: "ci@example.com"
repos:
  - name: example-project
    url: "git@github.com:user/example.git"
    branch: main
    command: "make test"
    # timeout: 1800          # optional, seconds
    # notify_email: override # optional, overrides default
```

### Step 3: Ansible scaffolding
- `ansible/inventory.yml`: localhost with local connection
- `ansible/ansible.cfg`: roles_path, vault_password_file at `/run/dropkick/vault-pass`
- `ansible/group_vars/all.yml`: variables for all roles (user, paths, laminar config, poll interval)

### Step 4: Role — base
1. Create `dropkick` system user (home: `/var/lib/dropkick`, shell: `/bin/bash`)
2. Create dirs: `~/.ssh`, `state/`, `/var/log/dropkick`, `/run/dropkick`
3. Deploy SSH key from vault to `~dropkick/.ssh/id_ed25519` (mode 0600)
4. Deploy SSH config with `StrictHostKeyChecking accept-new` (TOFU)
5. Install packages: `git`, `yq`, `curl`, `jq`, `ca-certificates`, `build-essential`
6. Install Docker (`docker.io` package), add `dropkick` and `laminar` users to `docker` group
7. Install Dagger (official install script to `/usr/local/bin/dagger`)

### Step 5: Role — laminar
1. Install build deps: `capnproto`, `cmake`, `g++`, `libboost-dev`, `libcapnp-dev`, `libsqlite3-dev`, `rapidjson-dev`, `zlib1g-dev`, `pkg-config`
2. Check if already installed at target version (skip build if so)
3. Clone laminar repo (shallow, specific tag/commit) to `/opt/laminar-build`
4. cmake + make + make install
5. Deploy `/etc/laminar.conf` from template (bind HTTP, RPC, title, keep_rundirs)
6. Enable + start `laminard.service`

Note: laminard runs as `laminar` user. Jobs run as `laminar` user (no privilege drop). The `dropkick` user triggers jobs via `laminarc` over the RPC socket.

### Step 6: Role — msmtp
1. Install `msmtp`, `msmtp-mta`
2. Deploy `/etc/msmtprc` with SMTP creds from vault (mode 0600)

### Step 7: Core scripts — poll.sh and notify.sh

**poll.sh** (~80 lines):
- `flock -n` on `/var/run/dropkick/poll.lock` to prevent concurrent runs
- Parse `/etc/dropkick/repos.yaml` with `yq`
- For each repo: `git ls-remote $url refs/heads/$branch`
- Compare against `/var/lib/dropkick/state/$name/last-commit`
- If changed: ensure workspace exists (clone or fetch), then `laminarc run $name`
- Update state file AFTER job completes (even on failure — prevents infinite retrigger)
- Log everything to `/var/log/dropkick/poll.log`

**notify.sh** (~20 lines):
- Accepts `--job`, `--run`, `--result`, `--to` args
- Sends email via `msmtp -t` with subject `[Dropkick CI] $JOB #$RUN: $RESULT`

### Step 8: Role — ci-poller
1. Copy `repos.yaml` to `/etc/dropkick/repos.yaml`
2. Deploy `poll.sh` to `/usr/local/bin/dropkick-poll`
3. Deploy `notify.sh` to `/usr/local/bin/dropkick-notify`
4. **Generate Laminar jobs** by looping over repos list:
   - `job.run.j2` -> `/var/lib/laminar/cfg/jobs/$name.run`: cd workspace, fetch, checkout, clean, run command (with optional timeout)
   - `job.after.j2` -> `/var/lib/laminar/cfg/jobs/$name.after`: call notify on failure
5. Clean up orphaned job scripts (repos removed from config)
6. Create state directories per repo
7. Deploy systemd service + timer, enable timer
8. Deploy logrotate config (daily, 14 rotations, compress)

### Step 9: Role — netbird
1. Install netbird via official install script/apt repo
2. Enable + start service
3. Print reminder to run `netbird up` manually once

### Step 10: Ansible playbook
- Pre-tasks: load repos.yaml, set facts
- Roles in order: base, laminar, msmtp, ci-poller, netbird

### Step 11: Ansible vault
- Create `vault/secrets.yml` with `ansible-vault create`
- Contains: `vault_ssh_private_key`, `vault_smtp_host`, `vault_smtp_user`, `vault_smtp_password`

### Step 12: PowerShell — setup.ps1
- `-New` mode: download Ubuntu rootfs, `wsl --import`, copy repo into VM, install Ansible, run playbook
- `-Update` mode: git pull inside VM, re-run playbook
- Default (no flag): update mode

### Step 13: PowerShell — startup.ps1
- Read vault password from Windows Credential Manager (`Get-StoredCredential -Target dropkick-vault`)
- Ensure WSL distro is running
- Pipe password to `/run/dropkick/vault-pass` (tmpfs, never persists)
- Register as Windows Task Scheduler task (runs at logon)

## Key Design Decisions

1. **State updated even on failure** — prevents broken commit from retriggering every 5 min. Failure is captured in Laminar UI + email.
2. **poll.sh does clone/fetch, .run does checkout** — separation of concerns. Poll manages the git state, Laminar job gets a clean working copy.
3. **`StrictHostKeyChecking accept-new`** — TOFU model. Accepts GitHub key on first contact, rejects changes. Right tradeoff for CI.
4. **Laminar runs as `laminar` user, polling as `dropkick` user** — dropkick triggers via laminarc RPC. Workspace owned by laminar (since jobs run as laminar). Poll.sh needs to clone into workspace, so `dropkick` user needs to be in `laminar` group with group-write on workspace dir.
5. **repos.yaml deployed to /etc/dropkick/** — stable path for poll.sh. Ansible also reads it at generation time for Laminar job files.
6. **flock in poll.sh only** — since polling is serialized, only one laminarc run happens at a time per poll cycle. No concurrent workspace access possible.

## Adding a New Project

1. Add entry to `config/repos.yaml`
2. Run `setup.ps1 -Update` (or `ansible-playbook playbook.yml` from inside VM)
3. Done — Laminar job is auto-generated, next poll picks it up

## Verification

1. Run `ansible-playbook playbook.yml` on a fresh Ubuntu VM/WSL
2. Verify services: `systemctl status laminard`, `systemctl status dropkick-poll.timer`
3. Add a test repo to repos.yaml, re-run Ansible
4. Force a poll: `sudo -u dropkick /usr/local/bin/dropkick-poll`
5. Check Laminar UI at http://localhost:8080 — should show the job run
6. Push a commit to the test repo, wait for next poll, verify it triggers
7. Introduce a failing test, verify email notification arrives
8. Run Ansible again — verify idempotency (no changes)
