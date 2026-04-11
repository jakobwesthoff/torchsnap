// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Title Bar
//
// Absolute overlay that owns the top chrome of an auxiliary
// window: the drag region plus the platform-appropriate
// window controls. Because it is positioned absolutely, the
// rest of the layout underneath is oblivious to its presence,
// and swapping the platform variant does not perturb any
// other component.
//
// Consumers render <TitleBar /> once at the root of a window
// that was built with `hide_native_chrome` on the Rust side
// (see ADR 0034). The component picks the right platform
// variant internally.
//
// The visual style of the macOS traffic lights is heavily
// inspired by github.com/agmmnn/tauri-controls — that project
// is the most thorough custom Tauri window-control
// implementation we found, and its colors, glyph paths, and
// Alt-key affordance are the closest match to native macOS.
// The component architecture (dispatcher + absolute overlay +
// platform abstraction) is ours.
// =========================================================

import { useEffect, useState, type ReactNode } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { cn } from "../lib/cn";

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
//
// The outer div is itself the `data-tauri-drag-region` — the
// traffic light buttons live inside as descendants. Tauri's
// drag-region handler inspects the click target's ancestors
// and short-circuits on interactive elements, so the buttons
// remain clickable while the rest of the strip drags the
// window. Structuring the drag region as a *sibling* of the
// buttons does not reliably work.
// =========================================================

function MacTitleBar() {
  const [isHovering, setIsHovering] = useState(false);
  const [isAltPressed, setIsAltPressed] = useState(false);

  // Track Alt so hovering the green button shows the zoom
  // (`+`) glyph instead of the fullscreen (two triangles)
  // glyph, matching native macOS behavior.
  useEffect(() => {
    const down = (e: KeyboardEvent) => {
      if (e.key === "Alt") setIsAltPressed(true);
    };
    const up = (e: KeyboardEvent) => {
      if (e.key === "Alt") setIsAltPressed(false);
    };
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
    };
  }, []);

  const handleClose = () => {
    void getCurrentWebviewWindow().close();
  };
  const handleMinimize = () => {
    void getCurrentWebviewWindow().minimize();
  };
  const handleZoom = () => {
    void getCurrentWebviewWindow().toggleMaximize();
  };
  const handleFullscreen = async () => {
    const win = getCurrentWebviewWindow();
    const fullscreen = await win.isFullscreen();
    await win.setFullscreen(!fullscreen);
  };

  return (
    <div
      data-tauri-drag-region
      className="absolute inset-x-0 top-0 z-50 h-12 select-none"
    >
      <div
        className="absolute left-[24px] top-[24px] flex items-center gap-[8px]"
        onMouseEnter={() => setIsHovering(true)}
        onMouseLeave={() => setIsHovering(false)}
      >
        <TrafficLight
          colorClass="bg-[#ff544d] active:bg-[#bf403a]"
          label="Close"
          onClick={handleClose}
        >
          {isHovering && <CloseGlyph />}
        </TrafficLight>
        <TrafficLight
          colorClass="bg-[#ffbd2e] active:bg-[#bf9122]"
          label="Minimize"
          onClick={handleMinimize}
        >
          {isHovering && <MinimizeGlyph />}
        </TrafficLight>
        <TrafficLight
          colorClass="bg-[#28c93f] active:bg-[#1e9930]"
          label={isAltPressed ? "Zoom" : "Toggle Fullscreen"}
          onClick={isAltPressed ? handleZoom : handleFullscreen}
        >
          {isHovering && (isAltPressed ? <PlusGlyph /> : <FullscreenGlyph />)}
        </TrafficLight>
      </div>
    </div>
  );
}

// =========================================================
// Traffic-light button
// =========================================================

interface TrafficLightProps {
  /** Tailwind classes carrying the default and active bg color. */
  colorClass: string;
  label: string;
  onClick: () => void;
  children: ReactNode;
}

function TrafficLight({ colorClass, label, onClick, children }: TrafficLightProps) {
  return (
    <button
      type="button"
      aria-label={label}
      onClick={onClick}
      className={cn(
        "flex h-3.5 w-3.5 cursor-default items-center justify-center rounded-full",
        "border border-black/[.12] dark:border-none",
        "text-black/60 dark:text-black",
        colorClass,
      )}
    >
      {children}
    </button>
  );
}

// =========================================================
// Glyphs
//
// SVG paths adapted from the agmmnn/tauri-controls icon set.
// Each glyph uses `currentColor` so the button's text color
// drives the fill.
// =========================================================

function CloseGlyph() {
  return (
    <svg width="6" height="6" viewBox="0 0 16 18" fill="none" xmlns="http://www.w3.org/2000/svg">
      <path
        d="M15.7522 4.44381L11.1543 9.04165L15.7494 13.6368C16.0898 13.9771 16.078 14.5407 15.724 14.8947L13.8907 16.728C13.5358 17.0829 12.9731 17.0938 12.6328 16.7534L8.03766 12.1583L3.44437 16.7507C3.10402 17.091 2.54132 17.0801 2.18645 16.7253L0.273257 14.8121C-0.0807018 14.4572 -0.0925004 13.8945 0.247845 13.5542L4.84024 8.96087L0.32499 4.44653C-0.0153555 4.10619 -0.00355681 3.54258 0.350402 3.18862L2.18373 1.35529C2.53859 1.00042 3.1013 0.989533 3.44164 1.32988L7.95689 5.84422L12.5556 1.24638C12.8951 0.906035 13.4587 0.917833 13.8126 1.27179L15.7267 3.18589C16.0807 3.53985 16.0925 4.10346 15.7522 4.44381Z"
        fill="currentColor"
      />
    </svg>
  );
}

function MinimizeGlyph() {
  return (
    <svg width="8" height="8" viewBox="0 0 17 6" fill="none" xmlns="http://www.w3.org/2000/svg">
      <path
        fillRule="evenodd"
        clipRule="evenodd"
        d="M1.47211 1.18042H15.4197C15.8052 1.18042 16.1179 1.50551 16.1179 1.90769V3.73242C16.1179 4.13387 15.8052 4.80006 15.4197 4.80006H1.47211C1.08665 4.80006 0.773926 4.47497 0.773926 4.07278V1.90769C0.773926 1.50551 1.08665 1.18042 1.47211 1.18042Z"
        fill="currentColor"
      />
    </svg>
  );
}

function FullscreenGlyph() {
  return (
    <svg width="6" height="6" viewBox="0 0 15 15" fill="none" xmlns="http://www.w3.org/2000/svg">
      <path
        fillRule="evenodd"
        clipRule="evenodd"
        d="M3.53068 0.433838L15.0933 12.0409C15.0933 12.0409 15.0658 5.35028 15.0658 4.01784C15.0658 1.32095 14.1813 0.433838 11.5378 0.433838C10.6462 0.433838 3.53068 0.433838 3.53068 0.433838ZM12.4409 15.5378L0.87735 3.93073C0.87735 3.93073 0.905794 10.6214 0.905794 11.9538C0.905794 14.6507 1.79024 15.5378 4.43291 15.5378C5.32535 15.5378 12.4409 15.5378 12.4409 15.5378Z"
        fill="currentColor"
      />
    </svg>
  );
}

function PlusGlyph() {
  return (
    <svg width="8" height="8" viewBox="0 0 17 16" fill="none" xmlns="http://www.w3.org/2000/svg">
      <path
        fillRule="evenodd"
        clipRule="evenodd"
        d="M15.5308 9.80147H10.3199V15.0095C10.3199 15.3949 9.9941 15.7076 9.59265 15.7076H7.51555C7.11337 15.7076 6.78828 15.3949 6.78828 15.0095V9.80147H1.58319C1.19774 9.80147 0.88501 9.47638 0.88501 9.07419V6.90619C0.88501 6.50401 1.19774 6.17892 1.58319 6.17892H6.78828V1.06183C6.78828 0.676375 7.11337 0.363647 7.51555 0.363647H9.59265C9.9941 0.363647 10.3199 0.676375 10.3199 1.06183V6.17892H15.5308C15.9163 6.17892 16.229 6.50401 16.229 6.90619V9.07419C16.229 9.47638 15.9163 9.80147 15.5308 9.80147Z"
        fill="currentColor"
      />
    </svg>
  );
}
