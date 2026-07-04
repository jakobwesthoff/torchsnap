# Host crate emits 53 clippy warnings; dead public API and unused imports accumulate

**Kind:** improvement
**Severity:** medium
**Area:** src-tauri/src (crate-wide; see location table)

## Problem

`cargo clippy --manifest-path src-tauri/Cargo.toml` (verified
2026-07-02, clippy for rustc 1.94.0) reports **53 warnings for
the lib** plus 9 test-only ones. The crate has no
clippy-clean baseline, so any *new* warning introduced by a
change drowns in the existing noise and CI/local runs cannot
gate on it.

Full message → location table from the verified run:

```
associated function `from_substring` is never used        src/unicode.rs:99
associated function `with_client` is never used           src/caps/http.rs:145
called `expect` on `self.component` after `is_some`       src/wasm/runtime/cached_component.rs:193
doc list item overindented (×5)                           src/wasm/manifest/permissions/command.rs:155-161
field `label` is never read                               src/gadgets/mod.rs:39
field `path` is never read                                src/network/website_metadata/favicon_store.rs:51
field `savepoint_counter` is never read                   src/storage/sql_storage.rs:269
field `sql_config` is never read                          src/wasm/bridge.rs:58
field `url` is never read                                 src/network/website_metadata/metadata.rs:45
fields `db_path` and `migrations` are never read          src/wasm/runtime/host/sql.rs:32
method `acquire` is never used                            src/wasm/runtime/cached_component.rs:127
method `is_recognized` is never used                      src/caps/path_resolver.rs:37
method `json` is never used                               src/network/http.rs:218
method `parent` is never used                             src/wasm/logging/spans.rs:322
method `root` is never used                               src/wasm/source.rs:158
method `score` is never used                              src/frecency/mod.rs:211
method `user_agent` is never used                         src/network/http.rs:151
methods `filesystem`, `command`, `sql_storage`,
  `website_metadata`, `frecency` are never used           src/caps/mod.rs:88
methods `id` and `log` are never used                     src/wasm/logging/spans.rs:395
methods `log` and `log_in_span` are never used            src/wasm/logging/spans.rs:247
methods `post`, `put`, `patch`, `delete` are never used   src/network/http.rs:90
methods `record`, `score`, `scores`,
  `apply_scores` are never used                           src/frecency/gadget_frecency.rs:44
methods `tail` and `is_empty` are never used              src/wasm/logging/storage.rs:38
methods `text` and `json` are never used                  src/network/http.rs:444
methods `transaction` and `execute_raw` are never used    src/storage/sql_storage.rs:377
redundant closure (×2)                                    src/wasm/runtime/host/website_metadata.rs:42-43
redundant pattern matching (`is_ok()`)                    src/caps/command.rs:423
collapsible `if` (×4)                                     src/caps/command.rs:306,307;
                                                          src/network/website_metadata/mod.rs:187;
                                                          src/wasm/runtime/cached_component.rs:360
&-then-deref expression (×3)                              src/network/website_metadata/fetch.rs:124,137,153
too many arguments (8/7)                                  src/lib.rs:1174
unused import: `anyhow::Context`                          src/wasm/interface_gate.rs:22
unused import: `Emitter`                                  src/lib.rs:27
unused import: `tauri::Manager`                           src/gadgets/clipboard/mod.rs:40
unused imports: `LauncherPanel as _`,
  `PlatformLauncherPanel`                                 src/control/handlers/launcher.rs:24
very complex type (×9)                                    src/caps/clipboard.rs:21,25;
                                                          src/caps/opener.rs:51-53,91-93;
                                                          src/network/website_metadata/mod.rs:230
useless `format!` (test code)                             src/wasm/runtime/mod.rs:497
useless `vec!` (test code)                                src/gadget_host.rs:1700
```

The dominant category is dead code (~30 of the 53): whole HTTP
verb helpers, frecency scoring methods, span-logging methods,
and SQL transaction helpers exist with no production caller.
Some of this is deliberately retained API surface, but unlike
`GraphemePositions::empty` (`src/unicode.rs:33`, which carries
a documented `#[allow(dead_code)]`), none of these carry an
allow or a justification, so intent is indistinguishable from
rot.

## Overlap with existing todos

Three entries in the table are already tracked as their own
findings and should be fixed through those todos, not blindly
allowed:

- `sql_config` never read →
  `host-wasm/01kwfz4kkaq7spwnm2ncket1gj-sqlconfig-vestigial.md`
- `expect` after `is_some` in `cached_component.rs` →
  `host-wasm/01kwfz4kkaq7spwnm2ncket1gf-cached-component-reacquire-fragility.md`
- dead filesystem denial-diagnostics code →
  `host-core/01kwg168a5spvs73hj63rfy3ty-fs-denial-diagnostics-dead-code.md`

## Impact

- New warnings (including correctness-relevant lints like the
  `expect`-after-`is_some` one) are invisible in the noise.
- ~30 dead-code items mislead readers about what the live API
  surface is (e.g. `network/http.rs` looks like a full REST
  client; only GET is actually used).
- 4 unused imports and 12 auto-fixable suggestions are pure
  hygiene debt.

## Suggested fix

1. Decide per dead-code item: delete, or keep with a documented
   `#[allow(dead_code)]` (the `unicode.rs:33` pattern).
2. Apply `cargo clippy --fix` for the 12 mechanical
   suggestions; fix the remaining style lints by hand.
3. Once clean, consider denying warnings in CI (e.g.
   `cargo clippy -- -D warnings`) so the baseline stays clean.

For contrast: `tsc --noEmit` and `eslint .` both pass with zero
diagnostics, and `cargo test` in `src-tauri` passes 822/822
(all verified 2026-07-02), so this is a Rust-lint-only gap.

The gadgets workspace (`gadgets/Cargo.toml`) is nearly clean —
3 style warnings from the same run:

```
collapsible `if`                    gadgets/zerotier/src/auth.rs:76
collapsible `if`                    gadgets/zerotier/src/lib.rs:528
block rewritable with `?` operator  gadgets/open-url/src/lib.rs:112
```
