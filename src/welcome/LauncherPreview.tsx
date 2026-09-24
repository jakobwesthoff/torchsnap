// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Launcher preview
//
// A still picture of the launcher for the welcome page: the search
// row with a typed query, three fuzzy-matched results with the
// matched letters in the accent color, and the footer hint, with
// Snappy perched on top. It is the static launcher preview from the
// torchsnap.app hero, rebuilt from the app's own components. The
// animated version is a separate todo
// (todos/product/features/01m3a6b1d9w8a0tec6y4qd9s1a-welcome-launcher-animation.md).
//
// Decorative only: hidden from assistive technology.
// =========================================================

import { Icon } from "../components/Icon";
import { KeyCap } from "../components/KeyCap";
import { Mascot } from "../components/Mascot";
import { cn } from "../lib/cn";

/** `m: true` marks a fuzzy-match hit. */
type Segment = { t: string; m?: boolean };

interface PreviewResult {
  title: Segment[];
  subtitle: string;
  icon: string;
  selected?: boolean;
}

const QUERY = "gho";

const RESULTS: PreviewResult[] = [
  {
    title: [{ t: "Gho", m: true }, { t: "stty" }],
    subtitle: "/Applications/Ghostty.app",
    icon: "heroicons:command-line",
    selected: true,
  },
  {
    title: [
      { t: "Tog" },
      { t: "g", m: true },
      { t: "le Dark / Li" },
      { t: "gh", m: true },
      { t: "t M" },
      { t: "o", m: true },
      { t: "de" },
    ],
    subtitle: "Switch system appearance between dark and light",
    icon: "heroicons:moon",
  },
  {
    title: [
      { t: "G", m: true },
      { t: "oogle C" },
      { t: "h", m: true },
      { t: "r" },
      { t: "o", m: true },
      { t: "me" },
    ],
    subtitle: "/Applications/Google Chrome.app",
    icon: "heroicons:globe-alt",
  },
];

export function LauncherPreview() {
  return (
    <div
      aria-hidden="true"
      className="relative mx-auto mt-[118px] w-full max-w-[440px] select-none"
    >
      <Mascot
        size={192}
        className="pointer-events-none absolute -top-[110px] left-1/2 z-10 h-[136px] w-[136px] -translate-x-1/2"
      />

      <div className="relative overflow-hidden rounded-2xl bg-surface shadow-[0_0_0_1px_var(--color-border),0_4px_16px_rgb(0_0_0/0.12),0_16px_48px_rgb(0_0_0/0.16)]">
        <div className="flex items-center gap-3 px-4 py-3">
          <Icon icon="heroicons:magnifying-glass" className="h-5 w-5 shrink-0 text-accent" />
          <span className="flex-1 text-base text-text-primary">{QUERY}</span>
          <KeyCap>Esc</KeyCap>
        </div>

        <div className="border-t border-border" />

        {RESULTS.map((result) => (
          <div
            key={result.subtitle}
            className={cn(
              "flex items-center gap-3 border-l-2 py-2 pr-4 pl-3",
              result.selected ? "border-accent bg-selection" : "border-transparent",
            )}
          >
            <div className="flex h-8 w-8 shrink-0 items-center justify-center">
              <Icon icon={result.icon} className="h-6 w-6 text-text-secondary" />
            </div>
            <div className="min-w-0 flex-1 text-left">
              <div className="truncate text-sm text-text-primary">
                {result.title.map((segment, i) =>
                  segment.m ? (
                    <span key={i} className="text-accent">
                      {segment.t}
                    </span>
                  ) : (
                    segment.t
                  ),
                )}
              </div>
              <div className="truncate text-xs text-text-muted">{result.subtitle}</div>
            </div>
          </div>
        ))}

        <div className="flex items-center gap-2 border-t border-border px-4 py-2 text-xs text-text-secondary">
          <KeyCap>↵</KeyCap>
          <span>Open</span>
        </div>
      </div>
    </div>
  );
}
