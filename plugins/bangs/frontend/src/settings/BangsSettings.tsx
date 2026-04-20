// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// BangsSettings — placeholder scaffold
//
// The real settings component lands in the follow-up commit
// that `git mv`s it from `src/plugins/bangs/BangsSettings.tsx`
// and rewires imports to the SDK hooks / components. This
// placeholder exists so the scaffold step produces a
// build-passing frontend bundle.
// =========================================================

import { Section } from "@torchsnap/plugin-sdk/components";
import "../../styles/settings.css";

export function BangsSettings() {
  return (
    <div className="flex flex-col gap-4">
      <Section title="Bangs">
        <p className="text-sm text-text-muted">
          Settings UI lands in the next commit.
        </p>
      </Section>
    </div>
  );
}
