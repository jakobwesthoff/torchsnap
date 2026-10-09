---
kind: question
status: open
area: [src/settings/sections/WebsiteMetadataSection.tsx]
---

# formatRetentionDays has a dead branch — was 90 days meant to display as unlimited?

## Problem
`src/settings/sections/WebsiteMetadataSection.tsx:17-20`:

```ts
function formatRetentionDays(days: number): string {
  if (days === 90) return "90d";
  return `${days}d`;
}
```

Both branches produce the identical string for `days === 90`; the
special case is dead code. The shape strongly suggests the slider
maximum (`max={90}`, `WebsiteMetadataSection.tsx:92`) was at some
point intended to display a distinct label ("∞" / "forever" /
"unlimited") — i.e. max = never expire — and the mapping was either
never finished or removed without deleting the branch.

## Impact
None at runtime today. The open question is behavioral: should the
top of the retention slider mean "keep forever"? The backend
retention sweep interprets `cacheTtlDays` literally (see
`todos/backend/metadata/01kwg1z574brza6n2ffjnhvg9f-retention-can-wipe-live-favicons.md`
for the related retention behavior), so if "unlimited" was the
intent, both the label and the backend semantics are missing.

## Suggested fix
Either delete the dead branch, or implement the presumed intent:
`if (days === 90) return "∞"` plus a backend interpretation that
skips expiry at the sentinel value. Needs a decision on which was
meant.
