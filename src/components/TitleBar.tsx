// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Title Bar
//
// Absolute overlay that owns the top chrome of an auxiliary
// window: the drag region plus the platform-appropriate window
// controls. Because it is positioned absolutely it never
// participates in the underlying layout — swapping the
// platform variant (macOS traffic lights, Windows caption
// buttons, Linux equivalents) does not shift any content.
//
// Consumers render <TitleBar /> once at the root of a window
// that was built with `hide_native_chrome` on the Rust side
// (see ADR 0034). The component picks the right platform
// variant internally.
// =========================================================

import type { ReactNode } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

// =========================================================
// Dispatcher
//
// Hardcoded to macOS for now. When adding Windows or Linux
// support, branch here on a platform detection of your choice
// (e.g. `@tauri-apps/plugin-os`'s `platform()`). Whichever
// variant is chosen, it must render an absolute overlay of
// the same shape so the rest of the app stays oblivious to
// platform differences.
// =========================================================

export function TitleBar() {
  return <MacTitleBar />;
}

// =========================================================
// macOS variant — three traffic lights at the left
// =========================================================

function MacTitleBar() {
  const handleClose = () => {
    void getCurrentWebviewWindow().close();
  };

  const handleMinimize = () => {
    void getCurrentWebviewWindow().minimize();
  };

  const handleFullscreen = async () => {
    const win = getCurrentWebviewWindow();
    const fullscreen = await win.isFullscreen();
    await win.setFullscreen(!fullscreen);
  };

  return (
    <div className="absolute inset-x-0 top-0 z-50 h-9 select-none">
      {/* Drag region — fills the strip behind the buttons. Tauri
          excludes interactive descendants (<button>) from drag
          behavior, so the traffic lights remain clickable. */}
      <div data-tauri-drag-region className="absolute inset-0" />

      {/* Traffic lights. The `group` class on the container drives
          the hover reveal of the close / minimize / fullscreen
          glyphs — hovering any circle shows them on all three,
          matching native macOS behavior. */}
      <div className="group absolute left-[13px] top-[13px] flex items-center gap-[8px]">
        <TrafficLight color="#ff5f57" label="Close" onClick={handleClose}>
          <CloseGlyph />
        </TrafficLight>
        <TrafficLight color="#febc2e" label="Minimize" onClick={handleMinimize}>
          <MinimizeGlyph />
        </TrafficLight>
        <TrafficLight color="#28c840" label="Toggle Fullscreen" onClick={handleFullscreen}>
          <FullscreenGlyph />
        </TrafficLight>
      </div>
    </div>
  );
}

// =========================================================
// Traffic-light button
// =========================================================

interface TrafficLightProps {
  color: string;
  label: string;
  onClick: () => void;
  children: ReactNode;
}

function TrafficLight({ color, label, onClick, children }: TrafficLightProps) {
  return (
    <button
      type="button"
      aria-label={label}
      onClick={onClick}
      className="flex h-3 w-3 items-center justify-center rounded-full"
      style={{ backgroundColor: color }}
    >
      {children}
    </button>
  );
}

// =========================================================
// Glyphs — revealed on group hover
//
// Each glyph is an SVG drawn in a 10×10 viewBox so the stroke
// math stays legible. Strokes are near-black with some alpha
// to match macOS's own glyphs, which are dark but not pure
// black.
// =========================================================

const glyphClasses =
  "h-2 w-2 opacity-0 transition-opacity duration-75 group-hover:opacity-100";
const glyphStroke = "rgba(0, 0, 0, 0.6)";

function CloseGlyph() {
  return (
    <svg viewBox="0 0 10 10" className={glyphClasses}>
      <path
        d="M2.5 2.5 L7.5 7.5 M7.5 2.5 L2.5 7.5"
        stroke={glyphStroke}
        strokeWidth="1.25"
        strokeLinecap="round"
      />
    </svg>
  );
}

function MinimizeGlyph() {
  return (
    <svg viewBox="0 0 10 10" className={glyphClasses}>
      <path
        d="M2 5 L8 5"
        stroke={glyphStroke}
        strokeWidth="1.25"
        strokeLinecap="round"
      />
    </svg>
  );
}

function FullscreenGlyph() {
  // Two small L-brackets at opposite corners, pointing outward —
  // a simplified take on the native "enter fullscreen" glyph.
  return (
    <svg viewBox="0 0 10 10" className={glyphClasses}>
      <path
        d="M2 4 L2 2 L4 2 M6 8 L8 8 L8 6"
        stroke={glyphStroke}
        strokeWidth="1.25"
        strokeLinecap="round"
        strokeLinejoin="round"
        fill="none"
      />
    </svg>
  );
}
