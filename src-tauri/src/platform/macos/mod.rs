// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod app_discovery;
pub(crate) mod cgimage_conversion;
mod clipboard;
mod launcher_panel;
mod settings_discovery;
mod tray;

pub use app_discovery::MdfindDiscovery;
pub use clipboard::MacosClipboard;
pub use launcher_panel::MacosLauncherPanel;
pub use settings_discovery::MacosSettingsDiscovery;
pub use tray::MacosTray;
