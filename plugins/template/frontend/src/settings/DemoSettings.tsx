// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// DemoSettings — Template Plugin Settings Component
//
// Rendered in the settings sidebar when the Template Plugin
// is selected. Demonstrates:
//
// - Reading and writing plugin settings via `usePluginSetting`
// - Tailwind styling matching host conventions
// - The settings component lifecycle
//
// The `usePluginSetting` hook is pre-bound to this plugin's
// namespace — calling `usePluginSetting("greeting")` reads
// and writes `plugins.template.greeting` in the store.
// =========================================================

import type { PluginSettingsProps } from "@torchsnap/plugin-sdk";
import "../../styles/settings.css";

export function DemoSettings({ usePluginSetting }: PluginSettingsProps) {
  const [greeting, setGreeting] = usePluginSetting<string>("greeting");

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-1.5">
        <label
          htmlFor="template-greeting"
          className="text-sm font-medium text-text-label"
        >
          Greeting Message
        </label>
        <input
          id="template-greeting"
          type="text"
          value={greeting ?? ""}
          onChange={(e) => setGreeting(e.target.value)}
          className="rounded-md border border-border-input bg-surface px-3 py-1.5 text-sm text-text-primary placeholder:text-text-muted focus:border-accent focus:outline-none"
          placeholder="Enter a greeting..."
        />
        <p className="text-xs text-text-muted">
          This setting is stored at <code>plugins.template.greeting</code> and
          demonstrates the <code>usePluginSetting</code> hook.
        </p>
      </div>
    </div>
  );
}
