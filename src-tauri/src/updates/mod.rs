// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Updates
//
// Torchsnap updates itself through `tauri-plugin-updater`. The feed
// URL and the public key of the update signature come from
// `plugins.updater` in `tauri.conf.json`; the plugin verifies every
// downloaded archive against that key before installing it.
//
// TODO: This is the first cut from the updater spike
// (todos/plans/01m39y23eygs16vteak59b2y8r-check-for-and-install-updates.md):
// a manual check from the tray that asks through native dialogs and
// installs right away. Scheduling, the settings keys, the location
// check and the update window replace the dialogs in later steps.
// =========================================================

use anyhow::Context;
use tauri::Url;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_updater::UpdaterExt;

/// Environment variable that points the updater at another feed, for
/// testing an update against a locally served feed. It works in every
/// build, and the signature check against the compiled-in public key
/// still applies to whatever that feed announces.
const FEED_OVERRIDE_VAR: &str = "TORCHSNAP_UPDATE_FEED";

/// Parse the feed override. An unset or empty variable means no
/// override; anything else has to be a URL.
fn parse_feed_override(value: Option<&str>) -> anyhow::Result<Option<Url>> {
    match value.map(str::trim) {
        None | Some("") => Ok(None),
        Some(raw) => raw
            .parse::<Url>()
            .map(Some)
            .with_context(|| format!("parse {FEED_OVERRIDE_VAR} value '{raw}' as a URL")),
    }
}

/// What a manual check ended with, for the message shown afterwards.
enum Outcome {
    UpToDate { current: String },
    Declined,
}

/// Check the feed, ask before installing, then install and restart.
///
/// Runs on its own thread: the native dialogs block until answered,
/// and the tray callback that starts this must return at once.
pub(crate) fn check_and_install_now(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(
        move || match tauri::async_runtime::block_on(check_and_install(&app)) {
            Ok(Outcome::UpToDate { current }) => {
                app.dialog()
                    .message(format!("Torchsnap {current} is the newest version."))
                    .title("You're up to date")
                    .kind(MessageDialogKind::Info)
                    .blocking_show();
            }
            Ok(Outcome::Declined) => {}
            Err(e) => {
                eprintln!("update check failed: {e:#}");
                app.dialog()
                    .message(format!("{e:#}"))
                    .title("Checking for updates failed")
                    .kind(MessageDialogKind::Error)
                    .blocking_show();
            }
        },
    );
}

async fn check_and_install(app: &tauri::AppHandle) -> anyhow::Result<Outcome> {
    let mut builder = app.updater_builder();
    if let Some(feed) = parse_feed_override(std::env::var(FEED_OVERRIDE_VAR).ok().as_deref())? {
        eprintln!("update feed overridden by {FEED_OVERRIDE_VAR}: {feed}");
        builder = builder
            .endpoints(vec![feed])
            .context("use the overridden update feed")?;
    }
    let updater = builder.build().context("create the updater")?;

    let Some(update) = updater.check().await.context("fetch the update feed")? else {
        return Ok(Outcome::UpToDate {
            current: app.package_info().version.to_string(),
        });
    };

    let install = app
        .dialog()
        .message(format!(
            "Torchsnap {} is available. You have {}.\n\nInstall it and restart Torchsnap?",
            update.version, update.current_version
        ))
        .title("Update available")
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Install and Restart".to_string(),
            "Later".to_string(),
        ))
        .blocking_show();
    if !install {
        return Ok(Outcome::Declined);
    }

    update
        .download_and_install(|_, _| {}, || {})
        .await
        .context("download and install the update")?;
    app.restart()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_or_blank_override_means_the_configured_feed() {
        assert!(parse_feed_override(None).unwrap().is_none());
        assert!(parse_feed_override(Some("  ")).unwrap().is_none());
    }

    #[test]
    fn override_is_parsed_as_url() {
        let url = parse_feed_override(Some("https://example.org/feed.json"))
            .unwrap()
            .expect("a set variable is an override");
        assert_eq!(url.as_str(), "https://example.org/feed.json");
    }

    #[test]
    fn malformed_override_is_an_error() {
        assert!(parse_feed_override(Some("not a url")).is_err());
    }
}
