// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Application Bundle Resolution (Fallback)
//
// Stub implementation. Returns no resolution until platform-
// specific bundle/identifier lookup is implemented for Linux
// and Windows.
// =========================================================

pub fn app_path_for_identifier(_identifier: &str) -> Option<String> {
    // TODO: Linux — resolve via .desktop file `Exec`/`Icon` lookup
    // TODO: Windows — resolve via registry App Paths / AppsFolder
    None
}

pub fn icon_image_for_path(_path: &str) -> anyhow::Result<Option<image::DynamicImage>> {
    Ok(None)
}
