// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Content Security Policy for guarded windows
//
// The update window renders release notes from the update feed, which
// is not signed. Its production HTML gets a CSP meta tag that allows
// scripts only from the bundle, plus the inline scripts the page
// itself carries (the theme preload), each pinned by its SHA-256.
// Nothing can be loaded from the network.
//
// The app-wide `security.csp` in `tauri.conf.json` stays unset, so
// the other windows are unaffected. Development builds get no policy:
// the Vite dev server injects inline scripts of its own.
// =========================================================

import { createHash } from "crypto";
import type { Plugin } from "vite";

const INLINE_SCRIPT = /<script(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/g;

/** SHA-256 source expressions for every inline script in `html`. */
export function inlineScriptHashes(html: string): string[] {
  return [...html.matchAll(INLINE_SCRIPT)].map(
    ([, body]) => `'sha256-${createHash("sha256").update(body, "utf8").digest("base64")}'`,
  );
}

export function contentSecurityPolicy(html: string): string {
  return [
    "default-src 'self'",
    ["script-src 'self'", ...inlineScriptHashes(html)].join(" "),
    // React sets inline style attributes; styles cannot run code.
    "style-src 'self' 'unsafe-inline'",
    "img-src 'self' data:",
    "font-src 'self' data:",
    // Tauri's IPC and events.
    "connect-src 'self' ipc: http://ipc.localhost",
    "object-src 'none'",
    "base-uri 'none'",
    "form-action 'none'",
    "frame-src 'none'",
  ].join("; ");
}

export function injectContentSecurityPolicy(html: string): string {
  const meta = `<meta http-equiv="Content-Security-Policy" content="${contentSecurityPolicy(html)}" />`;
  // The policy must come before any script it governs.
  return html.replace(/<head>/, `<head>\n    ${meta}`);
}

/** Vite plugin: add the policy to the named pages of a production build. */
export function guardedPagesCsp(pages: string[]): Plugin {
  return {
    name: "torchsnap-guarded-pages-csp",
    apply: "build",
    transformIndexHtml: {
      order: "post",
      handler(html, context) {
        const page = context.filename.split("/").pop() ?? "";
        return pages.includes(page) ? injectContentSecurityPolicy(html) : html;
      },
    },
  };
}
