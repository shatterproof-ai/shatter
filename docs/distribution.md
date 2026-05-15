# Distribution

Shatter publishes continuous GitHub prereleases. Those releases are development
builds, not semver releases. Pin exact build tags for required CI gates.

The example build tag used below is protected by the release-retention workflow:

```text
continuous-20260512-1735-abc123def456
```

## Pick One Owner

Install Shatter once per repository through the tool surface that already owns
repo-level automation. Language-specific jobs should call that one `shatter`
binary instead of adding Shatter to every language manifest.

| Repository shape | Recommended owner |
|---|---|
| Root `package.json` already owns repo tools | npm tarball dependency |
| Go-only repo with no root Node tooling | Go tool wrapper |
| CI-only or language-agnostic adoption | GitHub setup action |
| Rust-only repo | GitHub setup action in CI, pinned shell installer locally |

Cargo-native distribution is deferred. Rust users should not expect a `cargo
install` path yet.

For Shatter/Kapow-style polyglot repos, put Shatter in the root package manager
or setup workflow that already orchestrates cross-language checks. Keep
subproject manifests independent unless the subprojects are intentionally
versioned and updated independently.

## npm Tarball Dependency

Use this when a root `package.json` already owns developer tooling:

```json
{
  "devDependencies": {
    "@shatterproof/shatter": "https://github.com/shatterproof-ai/shatter/releases/download/continuous-20260512-1735-abc123def456/shatter-npm-wrapper.tgz"
  },
  "scripts": {
    "shatter": "shatter scan src/"
  }
}
```

Then run:

```bash
npm install
npm exec shatter -- --version
```

The wrapper package installs optional platform payload packages for supported
Linux and macOS targets, then forwards arguments to the selected `shatter`
binary.

## Go Tool Wrapper

Use this when a Go repo has no root Node tooling:

```bash
go get -tool github.com/shatterproof-ai/shatter/go-tool/cmd/shatter@continuous-20260512-1735-abc123def456
SHATTER_BUILD=continuous-20260512-1735-abc123def456 go tool shatter --version
```

The wrapper downloads `shatter-release.json`, verifies the matching archive
checksum, caches the payload under the user cache directory, and forwards
arguments to the real binary. Set `SHATTER_BUILD` or pass `--shatter-build` for
an exact binary build.

## GitHub Setup Action

Use this when the repository does not have one package-manager owner, or when
you only need Shatter in CI:

```yaml
steps:
  - uses: actions/checkout@v4
  - uses: shatterproof-ai/shatter@continuous-20260512-1735-abc123def456
    with:
      build: continuous-20260512-1735-abc123def456
  - run: shatter --version
  - run: shatter scan src/
```

The action installs the verified GitHub Release binary and adds it to `PATH`.
It does not run Docker images from the consuming repository.

## Dependency Updates

Prefer exact continuous build tags over floating `latest` values in CI. Renovate
is the best fit for registryless tarball URLs and cross-file grouping.

```json
{
  "regexManagers": [
    {
      "fileMatch": ["(^|/)package\\.json$"],
      "matchStrings": [
        "https://github\\.com/shatterproof-ai/shatter/releases/download/(?<currentValue>continuous-[^/]+)/shatter-npm-wrapper\\.tgz"
      ],
      "depNameTemplate": "shatterproof-ai/shatter",
      "datasourceTemplate": "github-releases"
    },
    {
      "fileMatch": ["(^|/)go\\.mod$"],
      "matchStrings": [
        "github\\.com/shatterproof-ai/shatter/go-tool/cmd/shatter\\s+(?<currentValue>[^\\s]+)"
      ],
      "depNameTemplate": "shatterproof-ai/shatter",
      "datasourceTemplate": "github-releases"
    },
    {
      "fileMatch": ["(^|/)\\.github/workflows/.*\\.ya?ml$"],
      "matchStrings": [
        "shatterproof-ai/shatter@(?<currentValue>continuous-[A-Za-z0-9.-]+)",
        "build:\\s*(?<currentValue>continuous-[A-Za-z0-9.-]+)"
      ],
      "depNameTemplate": "shatterproof-ai/shatter",
      "datasourceTemplate": "github-releases"
    }
  ],
  "packageRules": [
    {
      "matchPackageNames": ["shatterproof-ai/shatter"],
      "groupName": "shatter continuous build"
    }
  ]
}
```

Dependabot works well for the GitHub Action ref:

```yaml
version: 2
updates:
  - package-ecosystem: github-actions
    directory: /
    schedule:
      interval: weekly
```

Dependabot does not reliably update arbitrary GitHub Release tarball URLs in
`package.json`, so use Renovate for the npm tarball path.

## Release Retention

Continuous prereleases are retained with this policy:

- keep the 30 newest continuous prereleases
- keep one monthly checkpoint for the previous 12 months
- keep explicitly protected tags, including documented example pins

Pinned CI builds should use a protected or recent continuous tag. Older
unprotected continuous builds can be removed by the scheduled cleanup workflow.
