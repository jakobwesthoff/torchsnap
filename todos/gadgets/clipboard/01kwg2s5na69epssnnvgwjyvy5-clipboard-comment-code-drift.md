---
kind: improvement
severity: low
status: open
area: [src-tauri/src/gadgets/clipboard/]
---

# Clipboard gadget: three comment/code drift sites

## Problem

Three places where comments describe behavior the code does not
have (beyond the lifecycle drift covered in
`01kwg2s5na69epssnnvgwjyvy0-clipboard-enable-panics-state-never-cleared.md`):

1. **Format-priority comment contradicts the priority list.**
   `storage.rs:569-573`:
   ```rust
   /// Priority: image > files > text. Returns "text" as default.
   fn primary_format_from_csv(csv: Option<&str>) -> &'static str {
       const PRIORITY: &[&str] = &["files", "image", "html", "rtf", "text"];
   ```
   The doc says image outranks files; the code puts files first
   (and also handles html/rtf, which the doc omits). Whichever is
   intended, one of the two is wrong — note that
   `derive_display_text` (`formats.rs:183-207`) also prioritizes
   files over image, so the *comment* is most likely the stale
   side.

2. **Reference to a nonexistent rationale.** `formats.rs:160-162`:
   ```rust
   // Intentionally not capturing unknown formats — see
   // module-level comment for rationale.
   ContentFormat::Other(_) => None,
   ```
   The module-level comment (`formats.rs:5-24`) lists supported
   formats but contains no rationale for skipping unknown ones.
   Either add the rationale there or state it inline.

3. **`app_shutting_down` is set for plain disable too.** The field
   doc says "Flipped to true during app teardown. The lifecycle
   thread checks this to know when to exit entirely (vs. just
   stopping the watcher because `enabled` was toggled off)"
   (`mod.rs:76-79`) — but `disable()` sets it unconditionally for
   both cases (`mod.rs:340-347`), and `enable()` has to reset it
   (`mod.rs:321-325`). No behavioral difference today because
   every consumer checks `!running || app_shutting_down`
   (`watcher.rs:47-50`, `mod.rs:248`), which makes the flag
   redundant as implemented. Either give the flag its documented
   meaning (set only from the app-exit path) or remove it and use
   `running` alone.

## Suggested fix

Align each comment with the code (or vice versa) as described
above. Item 3 can shrink state: dropping `app_shutting_down`
simplifies `WatcherLifecycle` to `running` +
`watcher_shutdown`.
