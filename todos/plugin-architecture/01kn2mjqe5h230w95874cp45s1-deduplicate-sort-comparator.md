# Deduplicate Sort Comparator

The three-key sort `(score DESC, source ASC, id ASC)` is implemented
independently in four places:

1. Rust, catalog results (`plugin_host.rs:749-754`)
2. Rust, query plugin results (`plugin_host.rs:571-575`)
3. TypeScript, `compareEntries` in `useSearch.ts`
4. TypeScript, binary search comparator in `Launcher.tsx` (for selection
   stability)

The Rust side can be trivially deduplicated into a single function.

The TypeScript side has two copies — `compareEntries` and the inline comparator
in `Launcher.tsx`. The `Launcher.tsx` copy is the most dangerous: if
`compareEntries` changes but the binary search comparator is forgotten,
selection stability silently breaks. Extract one shared comparator and import
it in both places.

The cross-language duplication (Rust/TS) is harder to eliminate without code
generation, but should at least be documented as a coupling point with a
comment in both locations referencing the other.
