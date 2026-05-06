// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// DemoSettings — Template Gadget Settings Component
//
// Rendered in the settings sidebar when the Template Gadget
// is selected. Showcases the typical settings authoring
// surface so new gadget authors can copy-and-adapt:
//
// - `useGadgetInfo()` for the gadget's identity and reactive
//   `enabled` flag (used to disable controls when the gadget
//   is off)
// - `useGadgetSetting()` for namespaced reactive settings;
//   the gadget id is inferred from the surrounding
//   GadgetContextProvider
// - `Switch`, `Section`, `Entry` from the host's design-system
//   primitives, shared via the SDK
//
// This file is meant to be **read** by new gadget authors and
// adapted into real gadget code. End-to-end test coverage of
// the SDK shim machinery lives in the dedicated
// `gadgets/test-fixture/` crate, not here.
// =========================================================

import { useGadgetInfo, useGadgetSetting } from "@torchsnap/gadget-sdk/hooks";
import { Entry, Section, Switch } from "@torchsnap/gadget-sdk/components";
import "../../styles/settings.css";

export function DemoSettings() {
  const { enabled } = useGadgetInfo();
  const [greeting, setGreeting] = useGadgetSetting<string>("greeting");
  const [verbose, setVerbose] = useGadgetSetting<boolean>("verbose");

  return (
    <div className="flex flex-col gap-4">
      <Section title="Greeting">
        <div className="flex flex-col gap-1.5">
          <label htmlFor="template-greeting" className="text-sm font-medium text-text-label">
            Greeting Message
          </label>
          <input
            id="template-greeting"
            type="text"
            value={greeting ?? ""}
            onChange={(e) => setGreeting(e.target.value)}
            disabled={!enabled}
            className="rounded-md border border-border-input bg-surface px-3 py-1.5 text-sm text-text-primary placeholder:text-text-muted focus:border-accent focus:outline-none disabled:opacity-50"
            placeholder="Enter a greeting..."
          />
          <p className="text-xs text-text-muted">
            Stored at <code>gadgets.template.greeting</code>.
          </p>
        </div>
      </Section>

      <Section title="Behaviour">
        <Entry
          label="Verbose logging"
          description="Sample boolean setting wired to a Switch control"
        >
          <Switch checked={verbose} onChange={setVerbose} disabled={!enabled} />
        </Entry>
      </Section>
    </div>
  );
}
