// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Icon Infrastructure
//
// Platform-independent icon caching, processing, and
// identifier-based resolution. Icon extraction itself is
// platform-specific and lives in the `platform` module;
// everything here works with the decoded images those
// extractors produce.
// =========================================================

mod app_icon;
mod icon_cache;
mod icon_processing;

pub use app_icon::AppIconResolver;
pub use icon_cache::IconCache;
pub use icon_processing::process_icon;
