// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod launcher;
mod query;
mod status;

use super::handler::HandlerRegistry;

/// Register all built-in control API method handlers.
pub fn register_all(registry: &mut HandlerRegistry) {
    registry.register("show", launcher::ShowHandler);
    registry.register("hide", launcher::HideHandler);
    registry.register("toggle", launcher::ToggleHandler);
    registry.register("dismiss", launcher::DismissHandler);
    registry.register("query", query::QueryHandler);
    registry.register("status", status::StatusHandler);
}
