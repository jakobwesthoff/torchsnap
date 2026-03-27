// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { useSetting } from "../../hooks/useSetting";
import { ShortcutSection } from "../ShortcutSection";

export function GeneralSection() {
  const [globalShortcut, setGlobalShortcut] =
    useSetting<string>("globalShortcut");

  return (
    <div>
      <ShortcutSection
        globalShortcut={globalShortcut}
        setGlobalShortcut={setGlobalShortcut}
      />
    </div>
  );
}
