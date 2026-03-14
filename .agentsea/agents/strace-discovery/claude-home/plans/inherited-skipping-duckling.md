# Plan: Write tools/kapow/DATA_SOURCES.md

## Context
The kapow data pipeline tool needs comprehensive documentation of every data source it imports. This is a documentation-only task (no code changes). All source information has been gathered from `docs/plans/kapow-o0j-data-import-tool.md`, the kapow tool code, and beads issues.

## Action
Create `tools/kapow/DATA_SOURCES.md` with the following structure:

1. **Overview** — what the document covers, how to use `kapow` CLI
2. **Annual Import Schedule** — month-by-month calendar
3. **Data Sources** — one section per source with:
   - URL and access method
   - File format
   - Update cadence (expected + by dates)
   - Fetch command
   - Manual steps (if any)
   - Data fields imported
4. Sources to cover (19 total):
   - Urban Institute Education Data Portal (API aggregator for IPEDS, Scorecard, etc.)
   - College Scorecard CSV (direct download)
   - 9 Rankings: US News, Forbes, Niche, THE, QS, Washington Monthly, WSJ, ARWU, Money
   - NCAA Membership Directory
   - Köppen climate
   - ZIP coordinates (GeoNames)
   - FiveThirtyEight partisan lean
   - University hex colors (frishberg GitHub)
   - Application platform membership (Common App, Coalition, etc.)
   - Crime/Clery Act data

## Verification
- File renders well in a markdown previewer
- All 19 sources covered
- No code changes needed
