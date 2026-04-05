# Plugin SDK Crate

Create a `torchsnap-plugin-sdk` Rust crate that simplifies WASM plugin
development by wrapping the raw `wit_bindgen` output.

## Motivation

Currently hello-world (and any new plugin) uses raw
`wit_bindgen::generate!` and manually constructs WIT types. Every plugin
duplicates this boilerplate. An SDK crate would provide a clean,
documented API surface.

## Scope

- Re-export generated bindings under a clean, versioned API
- Convenience macro (e.g., `torchsnap_plugin!(MyPlugin)` instead of
  manual `export!` + trait impls)
- Ergonomic `Logger` struct wrapping the raw logging imports (similar to
  host-side `Logger`/`SpanBuilder`)
- Helpers for common patterns: JSON serialization for
  `ViewResponse::data`, `nucleo-matcher` integration for fuzzy scoring
- Possibly re-export `nucleo-matcher` and `serde_json` as optional
  features so plugins don't need to manage those dependency versions

## When

After converting 1-2 real plugins (calculator + one more) so we have
concrete usage patterns to inform the API design. Premature abstraction
would risk building the wrong API.

## References

- WIT definitions: `wit/torchsnap-plugin.wit`
- Hello-world plugin: `plugins/hello-world/` (current boilerplate baseline)
- Strategy doc: `docs/strategy/Selfcontained-Plugin-System.md`
