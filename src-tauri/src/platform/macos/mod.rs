// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod app_discovery;
mod app_resolver;
pub(crate) mod cgimage_conversion;
mod clipboard;
mod launcher_panel;
pub(crate) mod osascript;
mod settings_discovery;
mod tray;
mod window_chrome;

pub use app_discovery::MdfindDiscovery;
pub use app_resolver::{app_path_for_identifier, icon_image_for_path};
pub use clipboard::MacosClipboard;
pub use launcher_panel::MacosLauncherPanel;
pub use settings_discovery::MacosSettingsDiscovery;
pub use tray::MacosTray;
pub use window_chrome::MacosWindowChrome;
