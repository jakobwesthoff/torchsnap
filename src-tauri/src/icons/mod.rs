// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Icon Infrastructure
//
// Platform-independent icon caching and processing. The
// platform-specific extraction trait lives in `platform::
// icon_extraction`; everything here works with the decoded
// images that extractors produce.
// =========================================================

mod icon_cache;
mod icon_processing;

pub use icon_cache::IconCache;
