// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { createHash } from "crypto";
import { describe, expect, it } from "vitest";
import { contentSecurityPolicy, guardedPagesCsp, injectContentSecurityPolicy, inlineScriptHashes } from "./csp";

const THEME = "(function(){ document.documentElement.dataset.theme = 'dark'; })();";
const PAGE = `<!doctype html>
<html>
  <head>
    <script>${THEME}</script>
    <script type="module" crossorigin src="/assets/update-abc.js"></script>
  </head>
  <body></body>
</html>`;

function sha256(text: string): string {
  return `'sha256-${createHash("sha256").update(text, "utf8").digest("base64")}'`;
}

describe("inlineScriptHashes", () => {
  it("hashes inline scripts and skips ones loaded from a file", () => {
    expect(inlineScriptHashes(PAGE)).toEqual([sha256(THEME)]);
  });

  it("finds nothing in a page without inline scripts", () => {
    expect(inlineScriptHashes("<head><script src=/x.js></script></head>")).toEqual([]);
  });
});

describe("contentSecurityPolicy", () => {
  const policy = contentSecurityPolicy(PAGE);

  it("allows scripts from the bundle and the pinned inline script only", () => {
    expect(policy).toContain(`script-src 'self' ${sha256(THEME)};`);
    expect(policy).not.toContain("'unsafe-inline' ;");
    expect(policy).not.toMatch(/script-src[^;]*unsafe/);
  });

  it("loads nothing from the network", () => {
    expect(policy).toContain("default-src 'self'");
    expect(policy).toContain("img-src 'self' data:");
    expect(policy).not.toMatch(/https?:\/\/(?!ipc\.localhost)/);
  });

  it("keeps Tauri's IPC working", () => {
    expect(policy).toContain("connect-src 'self' ipc: http://ipc.localhost");
  });

  it("forbids plugins, frames, forms and base changes", () => {
    for (const directive of ["object-src 'none'", "frame-src 'none'", "form-action 'none'", "base-uri 'none'"]) {
      expect(policy).toContain(directive);
    }
  });
});

describe("injectContentSecurityPolicy", () => {
  it("puts the policy first in the head, before any script", () => {
    const html = injectContentSecurityPolicy(PAGE);
    const meta = html.indexOf('http-equiv="Content-Security-Policy"');
    expect(meta).toBeGreaterThan(html.indexOf("<head>"));
    expect(meta).toBeLessThan(html.indexOf("<script"));
  });
});

describe("guardedPagesCsp", () => {
  const plugin = guardedPagesCsp(["update.html"]);
  const transform = plugin.transformIndexHtml as {
    handler: (html: string, context: { filename: string }) => string;
  };

  it("only runs for production builds", () => {
    expect(plugin.apply).toBe("build");
  });

  it("adds the policy to a guarded page", () => {
    const html = transform.handler(PAGE, { filename: "/repo/update.html" });
    expect(html).toContain("Content-Security-Policy");
  });

  it("leaves other pages alone", () => {
    expect(transform.handler(PAGE, { filename: "/repo/settings.html" })).toBe(PAGE);
  });
});
