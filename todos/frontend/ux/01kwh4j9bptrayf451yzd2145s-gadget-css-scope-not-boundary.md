# Gadget CSS is injected into host `<head>` under `@scope`, which is not a containment boundary (a `}` escapes to global rules), but the risk is subsumed by trusted gadget JS

**Kind:** possible-bug (security / misleading framing)
**Severity:** low
**Area:** src/lib/gadgetCss.ts

## Problem
`injectGadgetCss` (`gadgetCss.ts:24-42`) fetches a gadget's CSS via
`torchsnap-gadget://` and injects it into the host webview's `<head>`,
wrapped in `@scope ([data-gadget="${gadgetId}"]) {\n${raw}\n}`, with the
body set via `style.textContent`. Call site: `wasmPluginLoader.ts:100-105`
(fire-and-forget on gadget registration); container:
`Launcher.tsx:94,119` (`<div data-gadget={gadgetId}>`). `textContent`
(not `innerHTML`) rules out any HTML/`</style>` vector.

## Impact

### `@scope` is not a containment boundary (three levels)
- **(a) Selector-scoping, not a sandbox.** `@scope` only constrains
  which elements a *selector* matches. It does nothing about resource
  loading: `background: url(…)`, `@font-face { src: url(…) }`,
  `@import`, `cursor: url(…)`, `list-style-image` all fire their
  network fetches whether scoped or not, and host design tokens on
  `:root` cascade into the scoped subtree by design. The doc comment's
  claim that scoping "ensures gadget styles cannot leak to the host"
  describes only the honest, well-formed case.
- **(b) Scoped rules reach host content inside the container.**
  `@scope (root)` with no `to (…)` limit applies to the root and all
  descendants, so any host-owned DOM rendered inside `[data-gadget]`
  would be styleable by the gadget. Today `GadgetViewProps` carry only
  data (`results`/`data`/`query`/`matchedPrefix`, `src/gadgets/types.ts:19-31`),
  so no host content sits inside the container — a latent concern that
  becomes live the moment the host renders shared chrome inside the
  gadget subtree.
- **(c) `}`-breakout — real, defeats `@scope` entirely with pure CSS.**
  CSS block boundaries are brace-matched in the tokenizer; the first
  unbalanced `}` terminates the block. For malicious
  `raw = "} :root { display: none } @media all {"`, the injected text is:
  ```css
  @scope ([data-gadget="evil"]) {
  } :root { display: none } @media all {
  }
  ```
  The `@scope` block is closed empty by the leading `}` of `raw`;
  parsing then continues at stylesheet top level, so
  `:root { display: none }` is a **global** sibling rule (blanks the
  whole app) and `@media all { }` is an empty sibling. The gadget has
  injected un-scoped, page-global CSS: fixed-position overlays over the
  launcher, hiding/spoofing real chrome, CSS-only UI-redress/clickjacking
  — no JS required. `@scope` provides zero containment against a
  malicious (or even accidentally brace-unbalanced) gadget.

### Exfiltration — real in isolation, subsumed here
CSS can beacon to arbitrary origins (`background:url()`,
`@font-face src:url()`, `@import`, `cursor:url()`), unconstrained
because `csp: null`, and can exfiltrate attribute values
(`input[value^="a"]{background:url(/leak?a)}`-style) within the scope,
or against the whole document after the `}`-breakout. **However this
grants the gadget principal nothing new:** the gadget's own JS bundle
is `import()`ed into the same host webview and runs at the host origin
with full DOM access, `fetch` (csp null), and Tauri IPC. Every CSS
attack here — global-rule injection, UI spoofing, beaconing, attribute
exfil — is a strict subset of what the JS does trivially. Even a
CSS-only gadget is authored by the same trust principal, who could
equally ship JS.

### `gadgetId` interpolation is not the vector
The `[a-z0-9-]` charset (non-empty, no leading/trailing hyphen,
`manifest/mod.rs:185-204`) makes selector-breakout via the id
impossible: none of `"`/`]`/`)`/`{`/`}`/`\`/whitespace can appear. The
injection vector is the CSS **body** (`raw`), not the id.
`CSS.escape(gadgetId)` is cosmetic under the current validation (no
`[a-z0-9-]` char needs escaping inside a double-quoted attribute-selector
string); keep it only as cheap defense-in-depth against a future
loosening of the charset.

### Severity: low
The gadget frontend JS is same-origin host-realm code with full DOM,
`fetch`, and Tauri IPC. The `}`-breakout is real and completely defeats
`@scope` as a boundary, but it confers no capability the gadget
principal lacks, so it does not raise severity. The governing risk is
the trust model (untrusted JS at host origin), not this wrapper. What
*is* wrong independently of severity: the code and its doc comment
present `@scope` as an isolation/containment mechanism ("gadget styles
cannot leak to the host or other gadgets"). That framing is false and
invites a future maintainer to rely on a boundary that does not exist.

## Suggested fix (two layers)
1. **The real boundary (architectural; track with the CSP/isolation
   track).** The only genuine containment is executing gadget frontend
   in an isolated context — a sandboxed iframe with a separate origin —
   because JS is the actual capability. Pair with a CSP that constrains
   resource fetches (`style-src`/`font-src`/`img-src`/`connect-src`) so
   `url()`/`@import`/`@font-face` beaconing is bounded. This is the
   correct home for the CSS issue too; track it with the CSP/asset todo
   rather than standalone. (Shadow DOM alone would contain the CSS — a
   `}`-breakout inside a `<style>` in a shadow root affects only that
   shadow tree — but it neither sandboxes the JS nor stops `url()`
   fetches, so it is not the whole answer.)
2. **Local hardening (cheap, optional; defense-in-depth /
   honest-mistake protection).** If injection into the host document is
   kept short-term:
   - **Correct the doc comment:** `@scope` here is cosmetic
     style-scoping for well-formed gadget CSS, **not** a security
     boundary; gadgets are trusted host-origin code. (This is the one
     standalone action worth doing now.)
   - Prefer moving the `<style>` into a Shadow DOM root on the gadget
     container so a stray or malicious `}` cannot reach the host
     document (also fixes the honest broken-build case).
   - If a full shadow-DOM refactor is out of scope now, do a
     **structural CSSOM verify**: parse the wrapped string
     (`new CSSStyleSheet().replaceSync(scoped)`) and reject unless the
     result is exactly one top-level rule (the `@scope` rule); the
     malicious input yields three top-level rules → rejected. This
     robustly detects breakout.
   - Add `CSS.escape(gadgetId)` (trivial, cosmetic today, guards future
     charset changes).
   - **Do NOT "reject `}`-bearing CSS."** Every real stylesheet is full
     of `}`; that check is unworkable. The correct robust test is
     structural (CSSOM rule-count / shadow-DOM isolation), not
     character-blacklisting.

## Related
- CSP + asset-protocol scope (the real home for the resource-fetch and
  isolation fixes):
  `../build/01kwh4j9bptrayf451yzd2145t-csp-null-asset-scope.md`.
- The architectural fact (gadget frontend = trusted host-origin code)
  is shared with the CORS-reflection finding
  (`../host-wasm/01kwh4j9bptrayf451yzd2145b-protocol-cors-origin-reflection.md`).
