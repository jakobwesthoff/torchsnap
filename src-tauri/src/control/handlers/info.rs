// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Info Handler
//
// Identifies the running app: process, build, where its data
// and control socket live, the tray icon's id and, on Linux,
// the display it is connected to. A client that runs several
// instances side by side (a desktop session and a headless
// one, each with its own data folder) tells them apart by
// these values.
// =========================================================

use std::path::PathBuf;

use serde::Serialize;
use serde_json::Value;
use tauri::{Manager, Runtime};

use crate::BuildInfo;
use crate::control::handler::{ControlError, Handler};
use crate::platform::TrayId;

pub struct InfoHandler;

impl Handler for InfoHandler {
    fn handle(&self, _params: Value, app: &tauri::AppHandle) -> Result<Value, ControlError> {
        let info = app_info(app)?;
        Ok(serde_json::to_value(info).expect("app info serializes"))
    }
}

/// The `info` answer.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    pid: u32,
    #[serde(flatten)]
    build: BuildInfo,
    data_dir: PathBuf,
    socket_path: PathBuf,
    /// `None` when no tray was built, which `setup()` does not allow;
    /// a test app has none.
    tray_id: Option<String>,
    /// `None` on platforms other than Linux.
    display: Option<DisplayInfo>,
}

#[derive(Debug, PartialEq, Serialize)]
struct DisplayInfo {
    /// `wayland` or `x11`; another GDK backend by its type name.
    backend: String,
    /// The display GDK connected to, such as `wayland-0` or `:0`.
    name: String,
}

fn app_info<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<AppInfo, ControlError> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| ControlError::Internal {
            message: format!("resolve app data dir: {e}"),
        })?;
    Ok(AppInfo {
        pid: std::process::id(),
        build: BuildInfo::CURRENT,
        data_dir,
        socket_path: crate::control::socket_path(app),
        tray_id: app.try_state::<TrayId>().map(|id| id.0.clone()),
        display: display(app)?,
    })
}

/// The display GDK opened for the app's windows. GDK is not thread
/// safe, so it is read on the main thread.
#[cfg(target_os = "linux")]
fn display<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<Option<DisplayInfo>, ControlError> {
    super::launcher::on_main_thread(app, || {
        use gdk::prelude::*;
        gdk::Display::default().map(|display| DisplayInfo {
            backend: display_backend(display.type_().name()),
            name: display.name().to_string(),
        })
    })
}

#[cfg(not(target_os = "linux"))]
fn display<R: Runtime>(_app: &tauri::AppHandle<R>) -> Result<Option<DisplayInfo>, ControlError> {
    Ok(None)
}

/// The name of the GDK backend whose display has GObject type
/// `type_name`.
#[cfg(any(target_os = "linux", test))]
fn display_backend(type_name: &str) -> String {
    match type_name {
        "GdkWaylandDisplay" => "wayland".to_string(),
        "GdkX11Display" => "x11".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};

    use super::*;

    fn app() -> tauri::App<MockRuntime> {
        mock_builder()
            .build(mock_context(noop_assets()))
            .expect("mock app builds")
    }

    #[test]
    fn info_identifies_the_process_and_build() {
        let app = app();
        let info = serde_json::to_value(app_info(app.handle()).expect("info")).expect("serializes");
        assert_eq!(info["pid"], serde_json::json!(std::process::id()));
        assert_eq!(info["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(info["gitHash"], env!("GIT_HASH"));
    }

    #[test]
    fn info_names_the_data_folder_and_the_socket_in_it() {
        let app = app();
        let info = app_info(app.handle()).expect("info");
        assert_eq!(
            info.data_dir,
            app.path().app_data_dir().expect("data dir resolves")
        );
        assert_eq!(info.socket_path, info.data_dir.join("control.sock"));
    }

    #[test]
    fn info_has_no_tray_id_without_a_tray() {
        let app = app();
        assert_eq!(app_info(app.handle()).expect("info").tray_id, None);
    }

    #[test]
    fn info_reports_the_tray_id() {
        let app = app();
        app.manage(TrayId("4242-1".into()));
        assert_eq!(
            app_info(app.handle()).expect("info").tray_id,
            Some("4242-1".to_string())
        );
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn info_has_no_display_outside_linux() {
        let app = app();
        let info = serde_json::to_value(app_info(app.handle()).expect("info")).expect("serializes");
        assert_eq!(info["display"], serde_json::Value::Null);
    }

    #[test]
    fn display_backend_names_wayland() {
        assert_eq!(display_backend("GdkWaylandDisplay"), "wayland");
    }

    #[test]
    fn display_backend_names_x11() {
        assert_eq!(display_backend("GdkX11Display"), "x11");
    }

    #[test]
    fn display_backend_keeps_another_backend_by_type_name() {
        assert_eq!(display_backend("GdkBroadwayDisplay"), "GdkBroadwayDisplay");
    }
}
