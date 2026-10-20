---
title: "Gadget CSS can break out of its @scope block into global rules"
kind: bug
component: frontend
impact: low
tags: [security, ux]
---
# Gadget CSS can break out of its @scope block into global rules

`injectGadgetCss` wraps a gadget's CSS in an `@scope` block and injects it into the host document. A `}` inside the gadget's CSS closes that block early, and the rest of the CSS lands at the top level as global rules. The code and its doc comment present `@scope` as a containment mechanism, which it is not. Severity is low because the gadget's frontend JavaScript already runs at the host origin with full DOM access, `fetch` and Tauri IPC, so the CSS breakout grants nothing the gadget does not already have.

## Problem

Read in the code, not reproduced.

`injectGadgetCss` (`injectGadgetCss` in `src/lib/gadgetCss.ts`) fetches a gadget's CSS via `torchsnap-gadget://` and injects it into the host webview's `<head>`, wrapped in `@scope ([data-gadget="${gadgetId}"]) {\n${raw}\n}`, with the body set via `style.textContent`. Call sites: `registerWasmGadget` in `src/gadgets/wasmPluginLoader.ts` (fire-and-forget on gadget registration), once for `frontend.launcherCss` and once for `frontend.settingsCss`. The settings CSS is scoped to the gadget's settings page since commit 083d2e6a (`src/settings/SettingsPanel.tsx`), but is wrapped in the same `@scope` block, so the breakout applies to it too. Container: `src/launcher/Launcher.tsx:93,118` (`<div data-gadget={gadgetId}>`). `textContent` (not `innerHTML`) rules out any HTML or `</style>` vector.

## Impact

### `@scope` is not a containment boundary (three levels)

- **(a) Selector scoping, not a sandbox.** `@scope` only constrains which elements a *selector* matches. It does nothing about resource loading: `background: url(…)`, `@font-face { src: url(…) }`, `@import`, `cursor: url(…)` and `list-style-image` all fire their network fetches whether scoped or not. Host design tokens on `:root` cascade into the scoped subtree by design. The doc comment's claim that scoping "ensures gadget styles cannot leak to the host" describes only the honest, well-formed case.
- **(b) Scoped rules reach host content inside the container.** `@scope (root)` with no `to (…)` limit applies to the root and all descendants, so any host-owned DOM rendered inside `[data-gadget]` would be styleable by the gadget. Today `GadgetViewProps` carry only data (`results`, `data`, `query`, `matchedPrefix`; `src/gadgets/types.ts:19-31`), so no host content sits inside the container. This is a latent concern that becomes live the moment the host renders shared chrome inside the gadget subtree.
- **(c) `}` breakout.** Real, and it defeats `@scope` entirely with pure CSS. CSS block boundaries are brace-matched in the tokenizer, and the first unbalanced `}` terminates the block. For malicious `raw = "} :root { display: none } @media all {"`, the injected text is:

  ```css
  @scope ([data-gadget="evil"]) {
  } :root { display: none } @media all {
  }
  ```

  The leading `}` of `raw` closes the `@scope` block empty. Parsing then continues at stylesheet top level, so `:root { display: none }` is a **global** sibling rule that blanks the whole app, and `@media all { }` is an empty sibling. The gadget has injected un-scoped, page-global CSS: fixed-position overlays over the launcher, hiding or spoofing real chrome, and CSS-only UI redress or clickjacking, with no JS required. `@scope` provides zero containment against a malicious gadget, or even against one with accidentally unbalanced braces.

### Exfiltration: real in isolation, subsumed here

CSS can beacon to arbitrary origins (`background:url()`, `@font-face src:url()`, `@import`, `cursor:url()`), unconstrained because `csp: null`. It can exfiltrate attribute values (`input[value^="a"]{background:url(/leak?a)}`-style) within the scope, or against the whole document after the `}` breakout. **However, this grants the gadget principal nothing new.** The gadget's own JS bundle is `import()`ed into the same host webview and runs at the host origin with full DOM access, `fetch` (CSP null) and Tauri IPC. Every CSS attack here (global-rule injection, UI spoofing, beaconing, attribute exfiltration) is a strict subset of what the JS does trivially. Even a CSS-only gadget is authored by the same trust principal, who could equally ship JS.

### `gadgetId` interpolation is not the vector

The `[a-z0-9-]` charset (non-empty, no leading or trailing hyphen, `manifest/mod.rs:185-204`) makes selector breakout via the id impossible: none of `"`, `]`, `)`, `{`, `}`, `\` or whitespace can appear. The injection vector is the CSS body (`raw`), not the id. `CSS.escape(gadgetId)` is cosmetic under the current validation, since no `[a-z0-9-]` character needs escaping inside a double-quoted attribute-selector string. Keep it only as cheap defense in depth against a future loosening of the charset.

### Severity: low

The gadget frontend JS is same-origin host-realm code with full DOM, `fetch` and Tauri IPC access. The `}` breakout is real and completely defeats `@scope` as a boundary, but it confers no capability the gadget principal lacks, so it does not raise the severity. The governing risk is the trust model (untrusted JS at host origin), not this wrapper. What is wrong independently of severity: the code and its doc comment present `@scope` as an isolation or containment mechanism ("gadget styles cannot leak to the host or other gadgets"). That framing is false, and it invites a future maintainer to rely on a boundary that does not exist.

## Suggested fix (two layers)

1. **The real boundary.** This is architectural and belongs to the CSP and isolation track. The only genuine containment is executing gadget frontend code in an isolated context, a sandboxed iframe with a separate origin, because JS is the actual capability. Pair it with a CSP that constrains resource fetches (`style-src`, `font-src`, `img-src`, `connect-src`), so `url()`, `@import` and `@font-face` beaconing is bounded. Track the CSS issue with the CSP and asset-protocol todo rather than standalone. A shadow DOM alone would contain the CSS, since a `}` breakout inside a `<style>` in a shadow root affects only that shadow tree. It neither sandboxes the JS nor stops `url()` fetches, so it is not the whole answer.
2. **Local hardening.** Cheap and optional; it protects against honest mistakes. If injection into the host document stays for now:
   - Correct the doc comment: `@scope` here is cosmetic style scoping for well-formed gadget CSS, not a security boundary. Gadgets are trusted host-origin code. This is the one standalone action worth doing now.
   - Prefer moving the `<style>` into a Shadow DOM root on the gadget container, so a stray or malicious `}` cannot reach the host document. This also fixes the honest broken-build case.
   - If a full shadow-DOM refactor is out of scope now, do a structural CSSOM verify. Parse the wrapped string (`new CSSStyleSheet().replaceSync(scoped)`) and reject it unless the result is exactly one top-level rule, the `@scope` rule. The malicious input yields three top-level rules and is rejected, so this detects breakout robustly.
   - Add `CSS.escape(gadgetId)`. It is trivial and cosmetic today, and it guards future charset changes.
   - Do not reject `}`-bearing CSS. Every real stylesheet contains `}`, so that check is unworkable. The robust test is structural (CSSOM rule count or shadow-DOM isolation), not character blacklisting.

## Relations

- Relates to: [Replace Origin reflection on the gadget asset protocol with an allowlist](01KWH4J9BPTRAYF451YZD2145B-replace-origin-reflection-on-the-gadget-asset-protocol-with-an-allowlist.md), both rest on the fact that gadget frontend is trusted host-origin code.
- Relates to: [Move the gadget runtime code into a src/gadget-runtime vertical](01KPK5JX0E1HWY0ARSFNSC0FCW-move-the-gadget-runtime-code-into-a-srcgadget-runtime-vertical.md), which moves `src/lib/gadgetCss.ts`.
