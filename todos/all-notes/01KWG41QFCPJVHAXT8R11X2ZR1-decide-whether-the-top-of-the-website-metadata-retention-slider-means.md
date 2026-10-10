---
title: "Decide whether the top of the website metadata retention slider means keep forever"
kind: investigation
component: frontend
status: needs-discussion
---
# Decide whether the top of the website metadata retention slider means keep forever

The website metadata retention slider in `src/settings/sections/WebsiteMetadataSection.tsx` shows its maximum of 90 days as `90d`, the same label as every other value. The dead `formatRetentionDays` branch for 90 suggests the maximum was meant to read as unlimited retention, which only the user can confirm. The question has no runtime effect today.

## Context

`src/settings/sections/WebsiteMetadataSection.tsx:17-20`:

```ts
function formatRetentionDays(days: number): string {
  if (days === 90) return "90d";
  return `${days}d`;
}
```

Both branches produce the identical string for `days === 90`; the
special case is dead code. The shape strongly suggests the slider
maximum (`max={90}`, `src/settings/sections/WebsiteMetadataSection.tsx:92`) was at some
point intended to display a distinct label ("∞" / "forever" /
"unlimited"), that is, max means never expire. The mapping was either
never finished or removed without deleting the branch.

## Impact

None at runtime today. The open question is behavioral: should the
top of the retention slider mean "keep forever"? The backend
retention sweep interprets `cacheTtlDays` literally (see the
retention todo under Relations for the related retention behavior),
so if "unlimited" was the intent, both the label and the backend
semantics are missing.

## Options

Either delete the dead branch, or implement the presumed intent:
`if (days === 90) return "∞"` plus a backend interpretation that
skips expiry at the sentinel value.

## Open questions

- Which was meant: a "keep forever" label at the slider maximum, or a
  branch that was removed without its label change? Only the user can
  decide.

## Relations

- Relates to: [Retention cleanup can delete favicons that are still referenced](01KWG1Z574BRZA6N2FFJNHVG9F-retention-cleanup-can-delete-favicons-that-are-still-referenced.md), the related retention behavior in the backend sweep.
