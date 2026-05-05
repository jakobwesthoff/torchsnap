// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Launcher entry barrel
//
// Vite builds this single file into `dist/launcher.js`; the
// host loads it via `torchsnap-plugin://` and looks up the
// named export that matches the manifest's
// `[frontend.views]` mapping — in our case
// `picker = "EmojiGrid"`.
//
// The launcher CSS is imported here (not from EmojiGrid)
// so vite emits exactly one `launcher.css` asset regardless
// of how many views this bundle ends up shipping in future.
// =========================================================

import "../styles/launcher.css";

export { EmojiGrid } from "./EmojiGrid";
