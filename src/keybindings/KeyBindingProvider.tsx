// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * KeyBinding Provider — Central registry and document-level keydown listener
 *
 * Maintains a registry of keybinding groups, a sorted match-order array,
 * and a single `document.addEventListener('keydown', ...)` listener.
 * First match wins; inactive bindings still consume the event.
 *
 * The `suppressed` prop allows a parent (e.g. ActionProvider) to disable
 * all keybinding dispatch while a modal or palette is open.
 */

import { useCallback, useEffect, useMemo, useRef, type ReactNode } from "react";
import { KeyBindingContext, type KeyBindingContextValue } from "./KeyBindingContext";
import { matchesCombo, isInputFocused, type KeyBindingDefinition, type KeyCombo } from "./matching";
import { platform } from "./platform";
import { createLogger } from "../lib/logger";

const logger = createLogger("keybindings");

export interface KeyBindingProviderProps {
  children: ReactNode;
  /** When true, the keydown listener short-circuits and no bindings fire. */
  suppressed?: boolean;
  /** When true, pressing Escape while an input element is focused blurs it
   *  instead of dispatching to keybindings. Default: false. */
  escapeBlursInput?: boolean;
}

// =========================================================
// Ctrl/Meta Collision Detection
//
// On Windows/Linux, Ctrl+Key and Meta+Key produce the same physical
// event (both are ctrlKey). When bindings for both exist on the same
// layer, one silently shadows the other — worth a dev-time heads-up.
// =========================================================

/**
 * Build a canonical key for a combo that strips the Ctrl/Meta distinction,
 * so we can detect when two combos would collide on non-macOS platforms.
 */
function collisionKey(combo: KeyCombo): string {
  const mods = combo.modifiers
    .filter((m) => m !== "Ctrl" && m !== "Meta")
    .sort()
    .join("+");
  return mods ? `${mods}+${combo.key}` : combo.key;
}

function warnCtrlMetaCollisions(bindings: KeyBindingDefinition[]): void {
  // Group by layer → collision key → which modifier variant(s) are present.
  const layers = new Map<number, Map<string, { ctrl: string[]; meta: string[] }>>();

  for (const def of bindings) {
    for (const kb of def.keybindings ?? []) {
      const hasCtrl = kb.combo.modifiers.includes("Ctrl");
      const hasMeta = kb.combo.modifiers.includes("Meta");
      if (!hasCtrl && !hasMeta) continue;

      let layerMap = layers.get(def.layer);
      if (!layerMap) {
        layerMap = new Map();
        layers.set(def.layer, layerMap);
      }

      const ck = collisionKey(kb.combo);
      let entry = layerMap.get(ck);
      if (!entry) {
        entry = { ctrl: [], meta: [] };
        layerMap.set(ck, entry);
      }

      if (hasCtrl) entry.ctrl.push(def.id);
      if (hasMeta) entry.meta.push(def.id);
    }
  }

  for (const [layer, layerMap] of layers) {
    for (const [ck, { ctrl, meta }] of layerMap) {
      if (ctrl.length > 0 && meta.length > 0) {
        logger.warn(
          `Ctrl/Meta collision on layer ${layer} for "${ck}": ` +
            `Ctrl bindings [${ctrl.join(", ")}] and Meta bindings [${meta.join(", ")}] ` +
            `are indistinguishable on Windows/Linux. The first match by order wins.`,
        );
      }
    }
  }
}

/** Match order: layer DESC (more specific first), order ASC, id ASC. */
function matchSort(a: KeyBindingDefinition, b: KeyBindingDefinition): number {
  if (a.layer !== b.layer) {
    return b.layer - a.layer;
  }
  const oa = a.order ?? 0;
  const ob = b.order ?? 0;
  if (oa !== ob) {
    return oa - ob;
  }
  return a.id.localeCompare(b.id);
}

export function KeyBindingProvider({
  children,
  suppressed = false,
  escapeBlursInput = false,
}: KeyBindingProviderProps) {
  const isMacOS = platform === "macos";

  // =========================================================
  // Registry
  // =========================================================

  // Map<groupId, KeyBindingDefinition[]> — one entry per useKeyBindings call.
  const registryRef = useRef(new Map<number, KeyBindingDefinition[]>());
  const nextGroupIdRef = useRef(0);

  // Sorted snapshot for matching (layer DESC).
  const matchBindingsRef = useRef<KeyBindingDefinition[]>([]);

  /** Rebuild sorted array from the registry. */
  const syncBindings = useCallback(() => {
    const all: KeyBindingDefinition[] = [];
    for (const group of registryRef.current.values()) {
      all.push(...group);
    }
    matchBindingsRef.current = all.sort(matchSort);

    // Warn about Ctrl+Key / Meta+Key collisions on the same layer.
    // On Windows/Linux both modifiers map to ctrlKey, making them
    // indistinguishable at runtime. The layer/order precedence still
    // resolves which handler fires, but the collision is likely
    // unintentional and worth flagging during development.
    if (import.meta.env.DEV) {
      warnCtrlMetaCollisions(all);
    }
  }, []);

  const register = useCallback(
    (bindingGroup: KeyBindingDefinition[]): (() => void) => {
      const groupId = nextGroupIdRef.current++;
      registryRef.current.set(groupId, bindingGroup);
      syncBindings();

      return () => {
        // Only remove if it's still the same group reference (not replaced).
        if (registryRef.current.get(groupId) === bindingGroup) {
          registryRef.current.delete(groupId);
          syncBindings();
        }
      };
    },
    [syncBindings],
  );

  // =========================================================
  // Global Keydown Listener
  // =========================================================

  useEffect(() => {
    function handleKeydown(event: KeyboardEvent) {
      if (suppressed) {
        return;
      }

      const inputFocused = isInputFocused();

      // When enabled, Escape while an input is focused blurs it and stops
      // propagation. This gives inputs a predictable dismiss gesture without
      // routing through the keybinding system.
      if (escapeBlursInput && inputFocused && event.key === "Escape") {
        (document.activeElement as HTMLElement).blur();
        event.preventDefault();
        return;
      }

      for (const binding of matchBindingsRef.current) {
        if (!binding.keybindings) {
          continue;
        }

        for (const kb of binding.keybindings) {
          const allowInInput = kb.allowInInput ?? false;

          if (inputFocused && !allowInInput) {
            continue;
          }

          if (matchesCombo(event, kb.combo, isMacOS)) {
            event.preventDefault();
            if (binding.active !== false) {
              binding.handler();
            }
            return; // First match wins; inactive still consumes
          }
        }
      }
    }

    document.addEventListener("keydown", handleKeydown);
    return () => document.removeEventListener("keydown", handleKeydown);
  }, [isMacOS, suppressed, escapeBlursInput]);

  // =========================================================
  // Context Value
  // =========================================================

  const contextValue: KeyBindingContextValue = useMemo(() => ({ register }), [register]);

  return <KeyBindingContext.Provider value={contextValue}>{children}</KeyBindingContext.Provider>;
}
