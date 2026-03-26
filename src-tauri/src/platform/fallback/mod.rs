// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod app_discovery;
mod icon_extraction;
mod launcher_panel;
mod tray;

pub use app_discovery::FallbackDiscovery;
pub use icon_extraction::FallbackIconExtractor;
pub use launcher_panel::FallbackLauncherPanel;
pub use tray::FallbackTray;
