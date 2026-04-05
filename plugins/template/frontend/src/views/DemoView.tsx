// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// DemoView — Template Plugin Launcher View
//
// Custom UI component rendered when the user types the "tpl:"
// prefix. Demonstrates:
//
// - JSX with the automatic runtime (no `import React` needed)
// - React hooks via the shim (`import { useState } from "react"`)
// - Tailwind utilities using the host's design tokens
// - Dark mode via inherited CSS custom properties
// - Reading `data` and `query` from PluginViewProps
// - Dismissing the launcher via `props.dismiss()`
// =========================================================

import { useState } from "react";
import type { PluginViewProps } from "@torchsnap/plugin";
import "../../styles/launcher.css";

export function DemoView({ data, query, dismiss }: PluginViewProps) {
  const [count, setCount] = useState(0);
  const echoText = (data as { query?: string })?.query ?? query ?? "";

  return (
    <div className="flex flex-col gap-4 p-6">
      <h2 className="text-lg font-semibold text-text-primary">
        Template Plugin
      </h2>

      <p className="text-sm text-text-secondary">
        This view was loaded dynamically from a WASM plugin's frontend
        bundle via the <code className="text-accent">torchsnap-plugin://</code> protocol.
      </p>

      {/* Echo the query to prove data flows from WASM → host → React */}
      <div className="rounded-lg border border-border bg-surface-inset p-4">
        <p className="text-xs font-medium text-text-muted uppercase tracking-wide mb-1">
          Query
        </p>
        <p className="text-sm text-text-primary font-mono">
          {echoText || "(empty)"}
        </p>
      </div>

      {/* Counter to prove useState works through the React shim */}
      <div className="flex items-center gap-3">
        <button
          onClick={() => setCount((c) => c + 1)}
          className="rounded-md border border-border bg-surface px-3 py-1.5 text-sm text-text-primary hover:bg-surface-hover transition-colors"
        >
          Count: {count}
        </button>
        <span className="text-xs text-text-muted">
          Proves React hooks work through the shim
        </span>
      </div>

      <button
        onClick={dismiss}
        className="self-start rounded-md bg-accent px-4 py-1.5 text-sm font-medium text-white hover:bg-accent-hover transition-colors"
      >
        Dismiss
      </button>
    </div>
  );
}
