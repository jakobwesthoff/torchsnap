// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod app_discovery;
mod app_resolver;
mod clipboard;
mod launcher_panel;
mod settings_discovery;
mod tray;
mod window_chrome;

pub use app_discovery::FallbackDiscovery;
pub use app_resolver::{app_path_for_identifier, icon_image_for_path};
pub use clipboard::FallbackClipboard;
pub use launcher_panel::FallbackLauncherPanel;
pub use settings_discovery::FallbackSettingsDiscovery;
pub use tray::FallbackTray;
pub use window_chrome::FallbackWindowChrome;
