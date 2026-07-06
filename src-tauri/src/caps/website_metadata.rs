// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// WebsiteMetadataCap
//
// Thin capability wrapping the shared WebsiteMetadataService.
// If this cap is present, the gadget declared
// `permissions.website-metadata = true`. The binary gate is
// encoded by the Option on ProvisionedCaps — no internal
// `enabled` flag needed.
// =========================================================

use std::sync::Arc;

use crate::network::website_metadata::{
    LookupError, LookupMode, LookupResult, WebsiteMetadataService,
};

#[derive(Debug, thiserror::Error)]
pub enum WebsiteMetadataCapError {
    #[error("invalid domain: {0}")]
    InvalidDomain(String),
}

pub struct WebsiteMetadataCap {
    service: Arc<WebsiteMetadataService>,
}

impl WebsiteMetadataCap {
    pub fn new(service: Arc<WebsiteMetadataService>) -> Self {
        Self { service }
    }

    pub fn lookup(
        &self,
        domain: &str,
        mode: LookupMode,
    ) -> Result<LookupResult, WebsiteMetadataCapError> {
        let result = tokio::task::block_in_place(|| self.service.lookup(domain, mode));
        match result {
            Ok(r) => Ok(r),
            Err(LookupError::InvalidDomain(d)) => Err(WebsiteMetadataCapError::InvalidDomain(d)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::notifier::SettingsNotifier;
    use tempfile::TempDir;

    fn make_cap() -> (WebsiteMetadataCap, TempDir, SettingsNotifier) {
        let tmp = TempDir::new().expect("temp dir");
        let notifier = SettingsNotifier::new();
        let svc = WebsiteMetadataService::new(tmp.path().to_path_buf(), &notifier, 30)
            .expect("construct service");
        (WebsiteMetadataCap::new(Arc::new(svc)), tmp, notifier)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn rejects_invalid_domain() {
        let (cap, _tmp, _notifier) = make_cap();
        let err = cap
            .lookup("https://example.com", LookupMode::Blocking)
            .unwrap_err();
        assert!(matches!(err, WebsiteMetadataCapError::InvalidDomain(_)));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn error_display_includes_domain() {
        let err = WebsiteMetadataCapError::InvalidDomain("bad.domain".into());
        assert!(err.to_string().contains("bad.domain"));
    }
}
