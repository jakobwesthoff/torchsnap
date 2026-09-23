# Opener capability: `open_path`/`reveal_path` have no root constraint; `open_url` `*` short-circuits before parse (re-enables `file://`/deep-links)

**Kind:** bug (security)
**Severity:** medium (open_path standalone, open_url `*`); high (open_path chained with a write/command grant); low (reveal_path)
**Area:** src-tauri/src/caps/opener.rs, src-tauri/src/wasm/manifest/permissions/opener.rs

## Problem
`OpenerCap` gates `open_path`/`reveal_path` on a plain boolean and
passes the path verbatim to the backend, and `open_url`'s `*` wildcard
short-circuits before URL parsing (`opener.rs:105-143`).

Backend chain, traced end to end: `OpenerCap` closure →
`tauri_plugin_opener` 2.5.3 → the `open` crate 5.3.3 → a spawned OS
binary. On macOS `Command::new("/usr/bin/open").arg(path)`
(`open-5.3.3/src/macos.rs:3-7`); on Linux `xdg-open`, then `gio open`
/ `gnome-open` / `kde-open` (`open-5.3.3/src/unix.rs:8-32`). The path
is passed as **exactly one argv token** — no shell, no second
argument. `open_path` runs `path.metadata()?` first
(`tauri-plugin-opener/src/open.rs:54-61`), so the path must already
exist and a `-flag` token fails the existence check (flag-injection
into `open` is effectively blocked). `open_url` has **no** existence
check (`open.rs:33-36`).

## Impact

### `open_path` ceiling (medium standalone)
What `/usr/bin/open` / `xdg-open` do with an existing path:
- `.app` bundle (macOS) → LaunchServices launches the app (runs its
  Mach-O). Real app launch.
- `.desktop` (Linux) → many environments execute its `Exec=` line.
  Real launch.
- A raw Unix executable (`/bin/sh`, an ELF/Mach-O) → **not** `exec`ed;
  handed to the default handler for that file type. Opening `/bin/sh`
  does not spawn a shell. So "which can launch executables" is
  accurate for bundles and `.desktop` launchers and misleading if read
  as "runs any binary."
- A document (pdf/html/docx) → opens in its default app (`.html` →
  browser, a mild local-file/phishing vector, not exec).

With a gadget holding `open-path = true` and no gadget-controlled file
on disk (established: gadget assets/WASM are served in-memory from the
archive, not extracted), the ceiling is: **launch any installed
application and open any existing file/document with its default
handler** — launch a password manager, open Mail composing a message,
launch System Settings, open any readable user document, on Linux
trigger an existing `.desktop`. Meaningful abuse, but **not arbitrary
code execution on its own**: the gadget can only open things that
already exist and cannot pass args to what it launches.

Chains that raise it to code execution (then **high**):
- `+ [permissions.filesystem]` write (if/when write lands) or
  `+ [[permissions.command]]` able to create a file → plant a
  malicious `.command`/`.desktop`/script/`.app`, then `open_path` it.
  `open_path` is the trigger; the write primitive is payload delivery.
- Any future asset-extraction-to-disk feature raises `open_path`'s
  ceiling automatically (the gadget would then control an on-disk
  file). Standing dependency to flag.

### `open_url` `*` wildcard (medium, footgun)
`check_scheme` returns `Ok(())` on `"*"` before `url::Url::parse`
(`opener.rs:129-132`), so:
- `file:///…` → opens an arbitrary existing local file — the exact
  vector ADR 0037 explicitly rejected ("An unrestricted opener lets
  any installed plugin open arbitrary URLs, including `file://`,
  `javascript:`, or deep-link schemes", `0037…md:79-83`). `*` silently
  re-enables it.
- Raw path, no scheme (`/Applications/Foo.app`) → `Url::parse` fails
  but `*` short-circuits, so `/usr/bin/open /Applications/Foo.app`
  launches the app.
- Custom/deep-link schemes (`zoommtg:`, `slack:`, `tel:`, `ms-…:`) →
  reach the OS handler and trigger actions in other apps.

Under `*`, `open_url` is a **strict superset of `open_path`** for local
content: it does everything `open_path` does, skips the existence
check, and adds the `file://` + deep-link surface. A gadget with
`schemes=["*"]` does not need `open-path=true` at all.

### `reveal_path` (low)
`reveal_item_in_dir` canonicalizes (path must exist) then selects the
item in a file-manager window (macOS `NSWorkspace`
`activateFileViewerSelectingURLs`, Linux `FileManager1 ShowItems`,
`reveal_item_in_dir.rs:12`). It only reveals; it does not launch or
open. Ceiling: confirm a path exists + pop a Finder/Files window at
it — information disclosure at most. Do not conflate with `open_path`.

### Severity framing
Both `open_path` and `open_url` are explicit install-time grants (the
user can see `open-path` / `schemes=["*"]` in the manifest). But
"consent" here is manifest-audit, not an interactive dialog (ADR 0037:
"no runtime prompts… audit… from the manifest alone", `:100-102`), so
the mitigation depends on the user actually reading the manifest —
weaker than an interactive per-permission prompt, though stronger than
the app-launcher proxy sibling which needs no grant at all.

**Consistency red flag (strong argument for the fix):** the `command`
subsystem *bans* opener-class binaries (`open`, `xdg-open`, `start`)
at manifest parse and redirects authors to `[permissions.opener]
open-path = true` (`command.rs:23-26`; `gadget-development.md:885-888`).
So the sanctioned replacement for a banned-as-dangerous primitive
applies **no root constraint at all**, while `command` itself offers a
`path-under` argv constraint that canonicalizes an argument under a
declared root. The safe path is less constrained than the thing it
replaced. Also: ADR 0037 promised `reveal-path` would get "Path
validation (no traversal, confined to safe roots)… its own design
pass" (`0037…md:64-72`); it shipped as a bare boolean with none of
that — the ADR's own commitment is unmet.

## Suggested fix
**`open_path` / `reveal_path` — add a root allowlist; drop the bare
boolean.** Mirror `[permissions.filesystem]`:
```toml
[permissions.opener]
open-path = ["${xdg-data}/myapp/exports/*", "/Applications/Calendar.app"]
```
Reuse the existing canonicalizing matcher — `caps/filesystem.rs`
already has `compile_fs_patterns` (glob `*`/`**`, `${…}` substitution,
prefix canonicalization) and `path_in_allowlist` (canonicalizes the
request path, resolving symlinks, before matching, `filesystem.rs:328-334`),
or reuse command's `path-under` primitive directly. Canonicalize the
request path and require a declared-root match before forwarding. This
brings `open_path` in line with the rest of the permission model (fs
read and command args are already root-scoped; opener is the outlier).
This is a breaking manifest change (bool → list); given pre-1.0 /
single-developer ownership, go straight to the list form. If a "launch
anything" tier is genuinely wanted, keep `open-path = true` as an
explicit dangerous tier whose consent surface is framed as "can launch
any installed application and open any file on your system," and stop
the docs implying it is a benign "open with the registered
application." Apply the same root list to `reveal_path` to honour ADR
0037's unmet promise (lower priority given its low ceiling).

**`open_url` — remove the pre-parse short-circuit and constrain `*`:**
- Always `Url::parse` first (reject unparseable), then check the
  scheme. `*` must mean "any *valid* scheme," never "any raw string."
  This kills raw-path/bare-string forwarding and the accidental
  `open_path`-equivalence.
- Hard-denylist dangerous schemes even under `*`: at minimum `file:`,
  ideally `javascript:` and `data:` too. Better still, drop `*` for
  `open_url` entirely and require explicit scheme enumeration — an
  opener author knows which schemes it opens, and `*` provides little
  legitimate value while silently defeating the allowlist ADR 0037
  designed.
- Fold in the sibling scheme-normalization fix
  (`01kwfz4kkaq7spwnm2ncket1gb-opener-schemes-unvalidated.md`):
  lowercase-normalize and validate scheme grammar (RFC 3986) at parse
  so the runtime check is plain equality.

Least-surprising secure design: root-scoped `open-path`/`reveal-path`
(list form, reusing the fs matcher) + `open_url` requiring parseable
URLs and explicit non-wildcard scheme enumeration with
`file:`/`javascript:`/`data:` denied unconditionally. This makes
opener consistent with `fs` (root-scoped) and `command` (path-under +
opener-binary ban), and closes the `*` bypass of the ADR's own threat
model.

## Caveats
- `open_path`/`open_url` pass a single argv token; no arg-injection
  into the launched app, and `open_path`'s existence check blocks
  `-flag` tokens. Do not overstate severity to "run app with attacker
  args."
- Standalone `open_path` is not arbitrary code exec today (no
  gadget-controlled on-disk file); it becomes code-exec only when
  chained with a write/command grant, or if asset-extraction-to-disk
  is ever added.
- `reveal_path` is reveal-only on existing paths — low; do not lump
  with `open_path`.
- macOS vs Linux differ: `.app` launch is macOS, `.desktop` execution
  is Linux; both open documents.

## Files
- `caps/opener.rs:105-143` (cap logic; `check_scheme` `*`
  short-circuit at `:130-132`)
- `tauri-plugin-opener-2.5.3/src/open.rs:33-61` (existence-check
  asymmetry: `open_path` checks, `open_url` doesn't)
- `open-5.3.3/src/macos.rs:3-7`, `open-5.3.3/src/unix.rs:8-32` (spawned
  binaries)
- `tauri-plugin-opener-2.5.3/src/reveal_item_in_dir.rs:12`
  (`reveal_path` is reveal-only)
- `wasm/manifest/permissions/opener.rs:24-75` (bool grants, no roots)
- `caps/filesystem.rs:185,328-334` (reusable `compile_fs_patterns` /
  `path_in_allowlist`)
- `wasm/manifest/permissions/command.rs:23-26,154-155,392-394`
  (opener-binary ban + `path-under` primitive)
- `docs/adr/0037-wasm-plugin-opener-api.md:79-83,64-72,100-102`
  (rejected `file://`/deep-links; unmet reveal-path root-validation
  promise)
- `docs/api/gadget-development.md:757-760,885-888` (doc framing to
  correct)

## Related
- Scheme normalization/validation at manifest parse:
  `01kwfz4kkaq7spwnm2ncket1gb-opener-schemes-unvalidated.md`.
- The app-launcher proxy (reaches `open_path` with no opener grant,
  but entry-store-gated):
  `../platform/01kwh4j9bptrayf451yzd2145r-app-launcher-forged-entry-id.md`.
