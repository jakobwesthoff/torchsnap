// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Host Keybindings Shim
//
// Bridges plugin code's
// `import { useKeyBindings, LAYER } from "@torchsnap/plugin-sdk/keybindings"`
// to the host-provided keybinding system on
// `window.__torchsnap.keybindings`.
//
// `useKeyBindings` registers component-lifecycle keybinding
// definitions through the host's KeyBindingProvider tree;
// `LAYER` is the well-known layer constants
// (`APP`, `VIEW`, `COMPONENT`) plugins use to declare priority.
//
// Same shim mechanism as the React, components, and hooks
// subpaths. Resolved at module evaluation time — the host's
// `initPluginSdk()` runs before any plugin bundle loads.
// =========================================================

// ---------------------------------------------------------
// Type mirrors (single source of truth lives in host)
// ---------------------------------------------------------

export interface KeyBinding {
  combo: {
    modifiers?: string[];
    key: string;
  };
  /** Allow the binding to fire while focus is in an `<input>`. */
  allowInInput?: boolean;
}

export interface KeyBindingDefinition {
  id: string;
  layer: number;
  order?: number;
  handler: () => void;
  /** When false the binding still consumes the event but the
   *  handler is a no-op. Defaults to true. */
  active?: boolean;
  keybindings?: KeyBinding[];
}

export interface LayerConstants {
  readonly APP: 0;
  readonly VIEW: 1;
  readonly COMPONENT: 2;
}

// ---------------------------------------------------------
// Ambient global
// ---------------------------------------------------------

// The `TorchsnapGlobal` interface is declared piecemeal
// across the SDK shims and merged by TypeScript (see
// `hooks.ts` for the full explanation).
declare global {
  interface TorchsnapGlobal {
    keybindings: {
      useKeyBindings: <T extends KeyBindingDefinition>(
        definitions: T[],
        changed?: (prev: T[], next: T[]) => boolean,
      ) => void;
      LAYER: LayerConstants;
    };
  }
  interface Window {
    __torchsnap?: TorchsnapGlobal;
  }
}

// ---------------------------------------------------------
// Re-exports
// ---------------------------------------------------------

const k = window.__torchsnap?.keybindings;
if (!k) {
  throw new Error(
    "@torchsnap/plugin-sdk/keybindings: window.__torchsnap is not initialized — call initPluginSdk() before loading plugin bundles",
  );
}

export const useKeyBindings = k.useKeyBindings;
export const LAYER = k.LAYER;
