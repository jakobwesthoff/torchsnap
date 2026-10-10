---
title: "open_path and reveal_path lack a root constraint, and the open_url wildcard skips URL parsing"
kind: bug
component: gadget-host
impact: high
tags: [security, capabilities]
---
# open_path and reveal_path lack a root constraint, and the open_url wildcard skips URL parsing

The opener capability gates `open_path` and `reveal_path` on a plain boolean and passes the path to the OS opener unchanged, so a granted gadget can launch any installed application and open any existing file. The `*` scheme wildcard for `open_url` short-circuits before URL parsing, which re-enables `file://` URLs and deep-link schemes that the opener ADR rejected. The fix is to root-scope the paths and to parse URLs before checking schemes.

## Problem
Read in the code, not reproduced.

`OpenerCap` gates `open_path` and `reveal_path` on a plain boolean and
passes the path verbatim to the backend. `open_url`'s `*` wildcard
short-circuits before URL parsing (`src-tauri/src/caps/opener.rs:105-143`).

Backend chain, traced end to end: the `OpenerCap` closure, then
`tauri_plugin_opener` 2.5.3, then the `open` crate 5.3.3, then a spawned OS
binary. On macOS this is `Command::new("/usr/bin/open").arg(path)`
(`open-5.3.3/src/macos.rs:3-7`). On Linux it is `xdg-open`, then `gio open`,
`gnome-open`, or `kde-open` (`open-5.3.3/src/unix.rs:8-32`). The path
is passed as **exactly one argv token**, with no shell and no second
argument. `open_path` runs `path.metadata()?` first
(`tauri-plugin-opener-2.5.3/src/open.rs:54-61`), so the path must already
exist, and a `-flag` token fails the existence check. Flag injection
into `open` is therefore effectively blocked. `open_url` has **no** existence
check (`open.rs:33-36`).

## Impact

### `open_path` ceiling (medium standalone)
What `/usr/bin/open` or `xdg-open` does with an existing path:
- A `.app` bundle (macOS): LaunchServices launches the app, which runs its
  Mach-O. This is a real app launch.
- A `.desktop` file (Linux): many environments execute its `Exec=` line.
  This is a real launch.
- A raw Unix executable (`/bin/sh`, an ELF or Mach-O) is **not** exec'ed. It is
  handed to the default handler for that file type. Opening `/bin/sh`
  does not spawn a shell. "Can launch executables" is accurate for bundles and
  `.desktop` launchers, and misleading if read as "runs any binary."
- A document (pdf, html, docx) opens in its default app. `.html` opens in
  the browser, which is a mild local-file or phishing vector, not an exec.

With a gadget holding `open-path = true` and no gadget-controlled file
on disk (gadget assets and WASM are served in-memory from the
archive, not extracted), the ceiling is: **launch any installed
application and open any existing file or document with its default
handler.** That means launching a password manager, opening Mail to compose a message,
launching System Settings, or opening any readable user document. On Linux it can also trigger an existing `.desktop`. This is meaningful abuse, but it is **not arbitrary
code execution on its own**. The gadget can only open things that
already exist, and it cannot pass arguments to what it launches.

Chains that raise it to code execution (then **high**):
- Adding a `[permissions.filesystem]` write (if and when write lands), or a
  `[[permissions.command]]` able to create a file, lets the gadget plant
  a malicious `.command`, `.desktop`, script, or `.app`, and then `open_path` it.
  `open_path` is the trigger, and the write primitive is payload delivery.
- Any future asset-extraction-to-disk feature raises `open_path`'s
  ceiling automatically, because the gadget would then control an on-disk
  file. This is a standing dependency to flag.

### `open_url` `*` wildcard (medium, a footgun)
`check_scheme` returns `Ok(())` on `"*"` before `url::Url::parse`
(`opener.rs:129-132`), so:
- `file:///...` opens an arbitrary existing local file. This is the exact
  vector ADR 0037 explicitly rejected ("An unrestricted opener lets
  any installed plugin open arbitrary URLs, including `file://`,
  `javascript:`, or deep-link schemes", `0037...md:79-83`). The `*` silently
  re-enables it.
- A raw path with no scheme (`/Applications/Foo.app`) fails `Url::parse`,
  but `*` short-circuits, so `/usr/bin/open /Applications/Foo.app`
  launches the app.
- Custom and deep-link schemes (`zoommtg:`, `slack:`, `tel:`, `ms-...:`) reach the OS handler and can trigger actions in other apps.

Under `*`, `open_url` is a **strict superset of `open_path`** for local
content. It does everything `open_path` does, skips the existence
check, and adds the `file://` and deep-link surface. A gadget with
`schemes=["*"]` does not need `open-path=true` at all.

### `reveal_path` (low)
`reveal_item_in_dir` canonicalizes its input, so the path must exist, and then selects the
item in a file-manager window (macOS `NSWorkspace`
`activateFileViewerSelectingURLs`, Linux `FileManager1 ShowItems`,
`reveal_item_in_dir.rs:12`). It only reveals. It does not launch or
open. The ceiling is confirming that a path exists and popping a Finder or Files window at
it, which is information disclosure at most. Do not conflate it with `open_path`.

### Severity framing
Both `open_path` and `open_url` are explicit install-time grants. The user can see `open-path` or `schemes=["*"]` in the manifest. But
"consent" here is manifest audit, not an interactive dialog (ADR 0037:
"no runtime prompts... audit... from the manifest alone", `:100-102`). The
mitigation therefore depends on the user actually reading the manifest. That is
weaker than an interactive per-permission prompt, though stronger than
the app-launcher proxy sibling, which needs no grant at all.

**Consistency red flag (a strong argument for the fix):** the `command`
subsystem *bans* opener-class binaries (`open`, `xdg-open`, `start`)
at manifest parse, and redirects authors to `[permissions.opener]
open-path = true` (`src-tauri/src/wasm/manifest/permissions/command.rs:23-26`;
`docs/api/gadget-development.md:885-888`). The sanctioned replacement for a banned-as-dangerous primitive
therefore applies **no root constraint at all**. Meanwhile `command` itself offers a
`path-under` argv constraint that canonicalizes an argument under a
declared root. The safe path is less constrained than the thing it
replaced. ADR 0037 also promised that `reveal-path` would get "Path
validation (no traversal, confined to safe roots)... its own design
pass" (`0037...md:64-72`). It shipped as a bare boolean with none of
that, so the ADR's own commitment is unmet.

## Suggested fix
**`open_path` and `reveal_path`: add a root allowlist and drop the bare
boolean.** Mirror `[permissions.filesystem]`:
```toml
[permissions.opener]
open-path = ["${xdg-data}/myapp/exports/*", "/Applications/Calendar.app"]
```
Reuse the existing canonicalizing matcher. `src-tauri/src/caps/filesystem.rs`
already has `compile_fs_patterns` (glob `*` and `**`, `${...}` substitution,
prefix canonicalization) and `path_in_allowlist`, which canonicalizes the
request path, resolving symlinks, before matching (`filesystem.rs:328-334`).
Alternatively, reuse command's `path-under` primitive directly. Canonicalize the
request path and require a declared-root match before forwarding. This
brings `open_path` in line with the rest of the permission model. Filesystem read and command arguments are already root-scoped, and opener is the outlier.
This is a breaking manifest change (bool to list). Given the pre-1.0 status and single-developer ownership, go straight to the list form. If a "launch
anything" tier is genuinely wanted, keep `open-path = true` as an
explicit dangerous tier whose consent surface is framed as "can launch
any installed application and open any file on your system." Stop the docs implying that it is a benign "open with the registered
application." Apply the same root list to `reveal_path` to honour ADR
0037's unmet promise (lower priority, given its low ceiling).

**`open_url`: remove the pre-parse short-circuit and constrain `*`.**
- Always `Url::parse` first (reject unparseable input), then check the
  scheme. `*` must mean "any *valid* scheme," never "any raw string."
  This kills raw-path and bare-string forwarding, and the accidental
  `open_path` equivalence.
- Hard-denylist dangerous schemes even under `*`: at minimum `file:`,
  and ideally `javascript:` and `data:` too. Better still, drop `*` for
  `open_url` entirely and require explicit scheme enumeration. An
  opener author knows which schemes it opens, and `*` provides little
  legitimate value while silently defeating the allowlist ADR 0037
  designed.
- Manifest parsing already validates the scheme grammar (RFC 3986)
  and lowercases schemes, so the runtime check can be a plain equality.

The least-surprising secure design is root-scoped `open-path` and `reveal-path`
(list form, reusing the fs matcher), plus `open_url` requiring parseable
URLs and explicit non-wildcard scheme enumeration, with
`file:`, `javascript:`, and `data:` denied unconditionally. This makes
opener consistent with `fs` (root-scoped) and `command` (path-under and
the opener-binary ban), and closes the `*` bypass of the ADR's own threat
model.

## Caveats
- `open_path` and `open_url` pass a single argv token. There is no arg injection
  into the launched app, and `open_path`'s existence check blocks
  `-flag` tokens. Do not overstate the severity as "run app with attacker
  args."
- Standalone `open_path` is not arbitrary code exec today, because no gadget-controlled file
  exists on disk. It becomes code exec only when
  chained with a write or command grant, or if asset-extraction-to-disk
  is ever added.
- `reveal_path` is reveal-only on existing paths, so it is low. Do not lump
  it with `open_path`.
- macOS and Linux differ. `.app` launch is macOS, and `.desktop` execution
  is Linux. Both open documents.

## Files
- `src-tauri/src/caps/opener.rs:105-143` (cap logic; `check_scheme` `*`
  short-circuit at `:130-132`)
- `tauri-plugin-opener-2.5.3/src/open.rs:33-61` (existence-check
  asymmetry: `open_path` checks, `open_url` doesn't)
- `open-5.3.3/src/macos.rs:3-7` and `open-5.3.3/src/unix.rs:8-32` (spawned
  binaries)
- `tauri-plugin-opener-2.5.3/src/reveal_item_in_dir.rs:12`
  (`reveal_path` is reveal-only)
- `src-tauri/src/wasm/manifest/permissions/opener.rs:24-75` (bool grants, no roots)
- `src-tauri/src/caps/filesystem.rs:185,328-334` (reusable `compile_fs_patterns` and
  `path_in_allowlist`)
- `src-tauri/src/wasm/manifest/permissions/command.rs:23-26,154-155,392-394`
  (opener-binary ban and `path-under` primitive)
- `docs/adr/0037-wasm-plugin-opener-api.md:79-83,64-72,100-102`
  (rejected `file://` and deep-links; unmet reveal-path root-validation
  promise)
- `docs/api/gadget-development.md:757-760,885-888` (doc framing to
  correct)

## Related
The app-launcher proxy todo, "app-launcher execute opens a path embedded in a stored command; forgery is blocked by an incidental entry-store gate, leaving a low-severity cross-gadget app-launch proxy", reaches `open_path` without any opener grant, but it is gated by the entry store. It sits in the platform component.
