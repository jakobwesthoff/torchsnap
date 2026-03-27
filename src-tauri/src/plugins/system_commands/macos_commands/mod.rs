// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// macOS System Commands Factory
//
// Assembles the full set of system commands available on macOS.
// =========================================================

mod appearance;
mod power;
mod utilities;

use super::SystemCommand;

pub fn system_commands() -> Vec<Box<dyn SystemCommand>> {
    vec![
        Box::new(power::LockScreen),
        Box::new(power::Sleep),
        Box::new(power::Restart),
        Box::new(power::Shutdown),
        Box::new(power::LogOut),
        Box::new(appearance::SwitchToDarkMode),
        Box::new(appearance::SwitchToLightMode),
        Box::new(utilities::EmptyTrash),
        Box::new(utilities::StartScreenSaver),
        Box::new(utilities::EjectDisc),
    ]
}
