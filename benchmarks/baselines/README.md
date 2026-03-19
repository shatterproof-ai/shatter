# Curated Perf Baselines

This directory is the only recommended tracked location for performance
artifacts in the repository.

Use it sparingly:

- Store only curated baseline snapshots.
- Good reasons to add a baseline include a release checkpoint, a major
  optimization milestone, or an intentionally refreshed golden benchmark.
- Do not commit routine local runs, CI histories, or exploratory profiling
  output here.

Expected workflow:

1. Run perf scenarios into an untracked directory such as `.shatter/perf-runs/`.
2. Compare the new artifacts against an existing baseline or recent CI run.
3. If the new run is intentionally becoming the blessed reference point, copy
   the minimal artifact set needed for future comparisons into this directory.
4. Document in the commit message or issue why the baseline was refreshed.
