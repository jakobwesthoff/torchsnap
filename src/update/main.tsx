// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { startWindow } from "../lib/startWindow";
import { UpdateApp } from "./UpdateApp";
import "../index.css";

// The update window reads no settings, so unlike Settings it does not
// load the settings store; its capability does not grant it either.
void startWindow({
  window: "update",
  app: () => <UpdateApp />,
});
