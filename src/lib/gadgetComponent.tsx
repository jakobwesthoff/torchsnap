// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Lightweight replacement for `React.lazy` that implements the Suspense
 * protocol directly. `React.lazy` always suspends on first render even
 * when the module is already cached, because its internal state machine
 * waits for the promise to resolve asynchronously. These helpers avoid
 * that extra render cycle by tracking resolution state ourselves.
 *
 * Two public constructors — `launcherComponent` and `settingsComponent`
 * — register their loaders in separate module-level arrays so each
 * entry point can preload only the components it actually renders.
 */

import { type ComponentType } from "react";

type ImportFactory<P> = () => Promise<{ default: ComponentType<P> }>;
type Loader = () => Promise<void>;

const launcherLoaders: Loader[] = [];
const settingsLoaders: Loader[] = [];

// =========================================================
// Inner factory (not exported)
// =========================================================

/**
 * Wrap a dynamic `import()` factory in a component that implements the
 * Suspense protocol: if the module has loaded, render it synchronously;
 * if not, throw the pending promise so a `<Suspense>` boundary can
 * show a fallback.
 *
 * Returns both the wrapper component and a `load` function that can be
 * called ahead of time to prime the module cache.
 */
function gadgetComponent<P extends object>(
  factory: ImportFactory<P>,
): { component: ComponentType<P>; load: Loader } {
  let Component: ComponentType<P> | null = null;
  let error: unknown = null;
  let promise: Promise<void> | null = null;

  const load = () => {
    if (!promise) {
      promise = factory().then(
        (mod) => {
          // A module without a component would leave the wrapper
          // suspended on an already-resolved promise forever, so it
          // counts as a failed load.
          if (mod.default == null) {
            error = new Error("gadget module resolved without a component");
            return;
          }
          Component = mod.default;
        },
        (err) => {
          error = err;
        },
      );
    }
    return promise;
  };

  const Wrapper = (props: P) => {
    if (error) throw error;
    if (Component) return <Component {...props} />;
    throw load();
  };

  return { component: Wrapper, load };
}

// =========================================================
// Public constructors
// =========================================================

/**
 * Create a dynamically loaded component for use in the launcher window.
 * Its loader is registered for `preloadLauncherComponents()`.
 */
export function launcherComponent<P extends object>(factory: ImportFactory<P>): ComponentType<P> {
  const { component, load } = gadgetComponent(factory);
  launcherLoaders.push(load);
  return component;
}

/**
 * Create a dynamically loaded component for use in the settings window.
 * Its loader is registered for `preloadSettingsComponents()`.
 */
export function settingsComponent<P extends object>(factory: ImportFactory<P>): ComponentType<P> {
  const { component, load } = gadgetComponent(factory);
  settingsLoaders.push(load);
  return component;
}

// =========================================================
// Preload triggers
// =========================================================

/**
 * Fire all launcher component imports so they resolve before React
 * first renders them. Call this once during launcher initialisation.
 */
export function preloadLauncherComponents(): void {
  for (const load of launcherLoaders) {
    load();
  }
}

/**
 * Fire all settings component imports so they resolve before React
 * first renders them. Call this once during settings initialisation.
 */
export function preloadSettingsComponents(): void {
  for (const load of settingsLoaders) {
    load();
  }
}
