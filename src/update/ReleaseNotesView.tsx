// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Release notes
//
// The notes come from the update feed, which is not signed (only the
// downloaded archive is), so they are rendered as untrusted Markdown:
//
// - `react-markdown` builds React elements and never injects HTML;
//   `skipHtml` also drops raw HTML from the text instead of showing
//   it as literal tags.
// - Images are not rendered, so the notes cannot load anything from
//   the network.
// - Links open in the default browser through the opener plugin and
//   only for http and https. The webview itself never navigates away
//   (the backend guards the window as well).
// =========================================================

import Markdown, { type Components } from "react-markdown";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { ReleaseNotes } from "./types";

function openInBrowser(href: string | undefined) {
  if (!href) {
    return;
  }
  let url: URL;
  try {
    url = new URL(href);
  } catch {
    return;
  }
  if (url.protocol === "https:" || url.protocol === "http:") {
    void openUrl(url.toString()).catch(() => {});
  }
}

const components: Components = {
  a: ({ href, children }) => (
    <a
      href={href}
      className="text-accent underline underline-offset-2 hover:text-accent-hover"
      onClick={(event) => {
        event.preventDefault();
        openInBrowser(href);
      }}
    >
      {children}
    </a>
  ),
  img: () => null,
  h1: ({ children }) => <h4 className="mt-3 mb-1 text-sm font-semibold">{children}</h4>,
  h2: ({ children }) => <h4 className="mt-3 mb-1 text-sm font-semibold">{children}</h4>,
  h3: ({ children }) => <h4 className="mt-3 mb-1 text-sm font-semibold">{children}</h4>,
  h4: ({ children }) => <h4 className="mt-3 mb-1 text-sm font-semibold">{children}</h4>,
  ul: ({ children }) => <ul className="ml-4 list-disc space-y-1">{children}</ul>,
  ol: ({ children }) => <ol className="ml-4 list-decimal space-y-1">{children}</ol>,
  p: ({ children }) => <p className="my-1">{children}</p>,
  code: ({ children }) => (
    <code className="rounded bg-surface-inset px-1 py-0.5 text-[12px]">{children}</code>
  ),
};

export function ReleaseNotesView({ releases }: { releases: ReleaseNotes[] }) {
  return (
    <div className="space-y-5 text-[13px] leading-relaxed text-text-secondary select-text">
      {releases.map((release) => (
        <section key={release.version} aria-label={`Torchsnap ${release.version}`}>
          <h3 className="text-sm font-semibold text-text-primary">
            Torchsnap {release.version}
            {release.date && (
              <span className="ml-2 font-normal text-text-muted">{release.date}</span>
            )}
          </h3>
          <Markdown skipHtml components={components}>
            {release.notes}
          </Markdown>
        </section>
      ))}
    </div>
  );
}
