// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Host Components Shim
//
// Bridges plugin code's
// `import { Switch } from "@torchsnap/plugin-sdk/components"`
// to the host-provided component implementations on
// `window.__torchsnap.components`.
//
// Same shim mechanism as `react`, `react/jsx-runtime`, and
// the plugin context hooks. The host's `initPluginSdk()`
// populates the global before any plugin bundle loads — by
// the time this module evaluates, the host references are
// already available.
//
// Bundling components through the global instead of inlining
// them into each plugin keeps host evolution in sync —
// fixing a `Switch` bug in the host immediately propagates
// to every plugin without rebuilding the plugins. Internal
// helpers like `cn` are bundled with the components in the
// host's main bundle and remain invisible to plugin authors.
// =========================================================

import type { ComponentType, ReactNode } from "react";

// ---------------------------------------------------------
// Component prop types (mirrors host source files)
// ---------------------------------------------------------

export interface SwitchProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  "aria-label"?: string;
  disabled?: boolean;
}

export interface SliderProps {
  value: number;
  onChange: (value: number) => void;
  min: number;
  max: number;
  step?: number;
  formatValue?: (value: number) => string;
  disabled?: boolean;
}

export interface SectionProps {
  title?: string;
  children: ReactNode;
  className?: string;
}

export interface EntryProps {
  label: string;
  description?: string;
  children: ReactNode;
}

// ---------------------------------------------------------
// Ambient global
// ---------------------------------------------------------

// The `TorchsnapGlobal` interface is declared piecemeal
// across the SDK shims and merged by TypeScript (see
// `hooks.ts` for the full explanation).
declare global {
  interface TorchsnapGlobal {
    components: {
      Switch: ComponentType<SwitchProps>;
      Slider: ComponentType<SliderProps>;
      Section: ComponentType<SectionProps>;
      Entry: ComponentType<EntryProps>;
    };
  }
  interface Window {
    __torchsnap?: TorchsnapGlobal;
  }
}

// ---------------------------------------------------------
// Re-exports
//
// Resolved at module evaluation time. The host calls
// `initPluginSdk()` before any plugin bundle is fetched, so
// `window.__torchsnap.components` is guaranteed to exist by
// the time this destructure runs. Same pattern as the React
// shim's hook destructure.
// ---------------------------------------------------------

const components = window.__torchsnap?.components;
if (!components) {
  throw new Error(
    "@torchsnap/plugin-sdk/components: window.__torchsnap is not initialized — call initPluginSdk() before loading plugin bundles",
  );
}

export const { Switch, Slider, Section, Entry } = components;
