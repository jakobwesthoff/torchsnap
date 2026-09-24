// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Which pages a guarded window may navigate to.
//!
//! The update and welcome windows show text that did not come from the
//! app bundle (release notes from the update feed). Their webviews may
//! only ever show the app's own pages: links open in the browser
//! through the opener plugin, and any navigation elsewhere is refused.

use tauri::Url;

/// True when `target` is one of the app's own pages: the bundled
/// frontend (`tauri://localhost`) or, in development, the dev server
/// the frontend is served from.
pub fn navigation_allowed(target: &Url, dev_url: Option<&Url>) -> bool {
    let bundled = target.scheme() == "tauri" && target.host_str() == Some("localhost");
    let dev_server = dev_url.is_some_and(|dev| dev.origin() == target.origin());
    bundled || dev_server
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(text: &str) -> Url {
        text.parse().expect("test URLs are valid")
    }

    #[test]
    fn bundled_pages_are_allowed() {
        assert!(navigation_allowed(
            &url("tauri://localhost/update.html"),
            None
        ));
    }

    #[test]
    fn dev_server_pages_are_allowed_in_development() {
        let dev = url("http://localhost:1420");
        assert!(navigation_allowed(
            &url("http://localhost:1420/update.html"),
            Some(&dev)
        ));
    }

    #[test]
    fn other_ports_of_the_dev_host_are_refused() {
        let dev = url("http://localhost:1420");
        assert!(!navigation_allowed(
            &url("http://localhost:8080/"),
            Some(&dev)
        ));
    }

    #[test]
    fn the_web_is_refused() {
        let dev = url("http://localhost:1420");
        assert!(!navigation_allowed(
            &url("https://torchsnap.app/"),
            Some(&dev)
        ));
        assert!(!navigation_allowed(&url("https://torchsnap.app/"), None));
    }

    #[test]
    fn localhost_without_a_dev_server_is_refused() {
        assert!(!navigation_allowed(&url("http://localhost:1420/"), None));
    }

    #[test]
    fn a_tauri_url_for_another_host_is_refused() {
        assert!(!navigation_allowed(&url("tauri://evil.example/"), None));
    }

    #[test]
    fn javascript_and_data_urls_are_refused() {
        assert!(!navigation_allowed(&url("javascript:alert(1)"), None));
        assert!(!navigation_allowed(&url("data:text/html,<p>x</p>"), None));
    }
}
