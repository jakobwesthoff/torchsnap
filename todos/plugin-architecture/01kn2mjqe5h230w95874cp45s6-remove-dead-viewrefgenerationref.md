# Remove Dead viewRefGenerationRef

In `useSearch.ts`, `viewRefGenerationRef` (line 99) is written to at line 195
but never read anywhere. The comment says "for external consumers if needed"
but no consumer exists.

This is vestigial infrastructure from an earlier design iteration. Remove it
to reduce confusion. If external generation tracking is needed in the future,
it can be re-added with an actual consumer.
