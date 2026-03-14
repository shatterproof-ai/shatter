# Plan: Remove duplicate institution name in detail card

## Context

The detail card modal (`InstitutionDetail.tsx`) renders the institution name twice:
1. As the `title` prop on the `<Modal>` component — shown in the modal header bar
2. As the first element inside the modal body in a `<Text size="lg">` tag

This causes visual clutter with the name appearing twice at the top of the detail view.

## Fix

**File:** `web/src/components/search/InstitutionDetail.tsx`

Remove the duplicate `<Text size="lg">` name element (line 191-193) from the header `<div>`, while keeping the city/state subtitle line — that info is not shown in the modal title.

**Before (lines 189-197):**
```tsx
{/* Header */}
<div>
  <Text size="lg" fw={700} c="brand.9">
    {inst.name}
  </Text>
  <Text size="sm" c="dimmed">
    {inst.city}, {inst.state}
  </Text>
</div>
```

**After:**
```tsx
{/* Header */}
<Text size="sm" c="dimmed">
  {inst.city}, {inst.state}
</Text>
```

The wrapping `<div>` is no longer needed since it only held the two `Text` elements; the city/state `Text` can stand alone within the `<Stack>`.

## Verification

```bash
cd /home/ketan/project/kapow/.claude/worktrees/worktree/detail-card-fix/web
pnpm build && pnpm lint && pnpm test
```

Check that any tests asserting on `inst.name` rendered twice in the modal body are updated to reflect it only appears in the modal title.
