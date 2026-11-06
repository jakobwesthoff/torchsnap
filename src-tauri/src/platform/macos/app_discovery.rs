// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Application Discovery (macOS)
//
// Discovers installed applications by querying the Spotlight
// index via the `mdfind` CLI and reading each app bundle's
// `Info.plist` for its display name and bundle identifier.
//
// Apps that set `LSUIElement` or `LSBackgroundOnly`, such as
// menu-bar apps, stay in the list. Users launch them on
// purpose, for example Alfred, Bartender or Screenshot.
//
// Spotlight also returns bundles nested inside other apps. A
// nested bundle stays when it sits directly in a folder named
// `Applications`, as Xcode's tools do in
// `Contents/Applications/`. Any other nested bundle, such as a
// login item or helper, is a part of its host app and is left
// out.
//
// Discovery that ends with no application is an error, never an
// empty list: the app launcher replaces its list and deletes its
// cached icons when discovery succeeds.
//
// An `mdfind` run that takes longer than 30 seconds fails, and
// the child is killed: a hung run would otherwise keep the app
// launcher from refreshing for good.
//
// The Mdfind struct encapsulates the raw subprocess call so
// the discovery logic stays focused on metadata extraction.
// =========================================================

use std::ffi::OsStr;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Context;
use image::DynamicImage;

use super::cgimage_conversion::nsworkspace_icon_for_file;
use crate::platform::app_discovery::{AppDiscovery, DiscoveredApp};

/// How long an `mdfind` run may take before it counts as failed.
const MDFIND_TIME_LIMIT: Duration = Duration::from_secs(30);

/// How often `output_within` looks at the child.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// How long `output_within` waits for a killed child to be reaped.
const REAP_GRACE: Duration = Duration::from_secs(1);

/// Read a child's pipe to its end on a thread of its own. A pipe
/// nobody reads fills up and blocks the child, which would look like
/// a hang.
fn drain(mut pipe: impl Read + Send + 'static) -> thread::JoinHandle<io::Result<Vec<u8>>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        pipe.read_to_end(&mut bytes)?;
        Ok(bytes)
    })
}

/// Run `command` to completion and return its output, or fail when
/// it is not done within `limit`.
///
/// The run is done when the child has exited and both of its pipes
/// have reached their end. A descendant that outlives the child and
/// keeps a pipe open therefore counts against the limit too, and the
/// call never returns partial output. On expiry the child gets
/// SIGKILL and `REAP_GRACE` to be reaped, so a child that ignores the
/// signal cannot hold the caller up past that.
fn output_within(command: &mut Command, limit: Duration) -> anyhow::Result<Output> {
    let program = command.get_program().to_string_lossy().into_owned();
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawn {program} process"))?;
    let stdout = drain(child.stdout.take().expect("stdout is piped"));
    let stderr = drain(child.stderr.take().expect("stderr is piped"));
    let deadline = Instant::now() + limit;

    let mut exit_status = None;
    loop {
        // A failing `try_wait` counts as a child still running: the
        // deadline then ends in the kill path, which reaps it.
        exit_status = exit_status.or(child.try_wait().ok().flatten());
        if let Some(status) = exit_status
            && stdout.is_finished()
            && stderr.is_finished()
        {
            // Both readers are finished, so joining cannot block.
            return Ok(Output {
                status,
                stdout: finished_bytes(stdout, &program)?,
                stderr: finished_bytes(stderr, &program)?,
            });
        }
        if Instant::now() >= deadline {
            break;
        }
        thread::sleep(POLL_INTERVAL);
    }

    let _ = child.kill();
    let (reaped_tx, reaped_rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = child.wait();
        let _ = reaped_tx.send(());
    });
    let _ = reaped_rx.recv_timeout(REAP_GRACE);

    anyhow::bail!("{program} did not finish within {limit:?}")
}

/// The bytes of a reader that has finished.
fn finished_bytes(
    reader: thread::JoinHandle<io::Result<Vec<u8>>>,
    program: &str,
) -> anyhow::Result<Vec<u8>> {
    reader
        .join()
        .map_err(|_| anyhow::anyhow!("reader thread of {program} panicked"))?
        .with_context(|| format!("read {program} output"))
}

/// Run a query command and return one path per output line.
fn query_command(command: &mut Command, limit: Duration) -> anyhow::Result<Vec<PathBuf>> {
    let program = command.get_program().to_string_lossy().into_owned();
    let output = output_within(command, limit).context("run the query")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("{program} exited with {}: {stderr}", output.status);
    }

    let stdout = String::from_utf8(output.stdout)
        .with_context(|| format!("decode {program} output as UTF-8"))?;

    Ok(stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect())
}

/// Thin wrapper around the macOS `mdfind` Spotlight CLI.
struct Mdfind;

impl Mdfind {
    /// Run a Spotlight query and return one path per result line.
    fn query(predicate: &str) -> anyhow::Result<Vec<PathBuf>> {
        query_command(Command::new("mdfind").arg(predicate), MDFIND_TIME_LIMIT)
    }
}

/// Whether an app path has an `Applications` folder anywhere above
/// the bundle.
///
/// This covers `/Applications/`, `/System/Applications/`,
/// `~/Applications/` and `/System/Library/CoreServices/Applications/`
/// (Keychain Access). It also admits the `Contents/Applications/`
/// folder of any app, which lists Finder's AirDrop although Finder
/// itself sits outside an Applications folder.
fn is_in_applications_dir(path: &Path) -> bool {
    path.to_string_lossy().contains("/Applications/")
}

/// Whether a bundle is embedded in another app as one of its parts.
///
/// Each `.app` bundle below the outermost one must sit directly in
/// a folder named `Applications`. A login item in
/// `Contents/Library/LoginItems/` or a helper in
/// `Contents/Frameworks/` fails that, and so does any app nested
/// inside such a helper.
///
/// `path` ends in the bundle's own `.app` component, as the
/// Spotlight content-type query returns.
fn is_embedded_helper(path: &Path) -> bool {
    let components: Vec<&OsStr> = path.components().map(|c| c.as_os_str()).collect();
    let bundle_indices: Vec<usize> = components
        .iter()
        .enumerate()
        .filter(|(_, name)| Path::new(name).extension() == Some(OsStr::new("app")))
        .map(|(index, _)| index)
        .collect();

    bundle_indices.windows(2).any(|pair| {
        let (outer, inner) = (pair[0], pair[1]);
        let parent = inner - 1;
        parent == outer || components[parent] != "Applications"
    })
}

/// Read an app bundle's `Contents/Info.plist` and build its
/// `DiscoveredApp`. Whether the bundle is listed at all is decided
/// by `apps_from_bundle_paths`.
fn try_discover_app(path: &Path) -> anyhow::Result<DiscoveredApp> {
    let plist_path = path.join("Contents/Info.plist");
    let info: plist::Dictionary = plist::from_file(&plist_path).context("read Info.plist")?;

    // Display name resolution order:
    //   1. CFBundleDisplayName: the localized user-facing name
    //   2. CFBundleName: shorter internal name
    //   3. Filename minus .app: last resort fallback
    let name = info
        .get("CFBundleDisplayName")
        .and_then(|v| v.as_string())
        .or_else(|| info.get("CFBundleName").and_then(|v| v.as_string()))
        .map(String::from)
        .unwrap_or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string())
        });

    let bundle_id = info
        .get("CFBundleIdentifier")
        .and_then(|v| v.as_string())
        .map(String::from);

    let id = path.to_string_lossy().into_owned();

    Ok(DiscoveredApp {
        id,
        name,
        path: path.to_owned(),
        bundle_id,
        icon_path: None,
    })
}

/// Build the app list from the bundle paths Spotlight returned.
fn apps_from_bundle_paths(paths: &[PathBuf]) -> Vec<DiscoveredApp> {
    let mut apps = Vec::with_capacity(paths.len());

    for path in paths {
        // Only index apps in known application directories.
        // This skips system agents in /System/Library/CoreServices/,
        // framework helpers, and other non-user-facing bundles.
        if !is_in_applications_dir(path) {
            continue;
        }

        // A bundle nested in another app outside an Applications
        // folder is a part of that app, such as a login item or
        // helper.
        if is_embedded_helper(path) {
            continue;
        }

        match try_discover_app(path) {
            Ok(app) => apps.push(app),
            // Individual parse failures should not abort the
            // entire discovery. Log and continue.
            Err(e) => {
                eprintln!("skipping {}: {e:#}", path.display());
            }
        }
    }

    apps
}

/// Discovers macOS applications via Spotlight (`mdfind`) and
/// `Info.plist` metadata parsing.
pub struct MdfindDiscovery;

/// Spotlight query matching every application bundle.
const APP_BUNDLE_QUERY: &str = "kMDItemContentType == 'com.apple.application-bundle'";

/// Run `query` for the application bundle paths and build the app
/// list from them.
///
/// A list without a single app is an error. The caller replaces its
/// app list and deletes its cached icons on `Ok`, so an empty `Ok`
/// would wipe both.
///
/// Taking the query as a parameter lets tests answer it without
/// running `mdfind`.
fn discover_from(
    query: impl FnOnce() -> anyhow::Result<Vec<PathBuf>>,
) -> anyhow::Result<Vec<DiscoveredApp>> {
    let paths = query().context("query Spotlight for application bundles")?;
    let apps = apps_from_bundle_paths(&paths);

    if apps.is_empty() {
        anyhow::bail!(
            "no application bundle left of {} Spotlight results",
            paths.len()
        );
    }

    Ok(apps)
}

impl AppDiscovery for MdfindDiscovery {
    fn discover(&self) -> anyhow::Result<Vec<DiscoveredApp>> {
        discover_from(|| Mdfind::query(APP_BUNDLE_QUERY))
    }

    fn icon(&self, app: &DiscoveredApp) -> anyhow::Result<Option<DynamicImage>> {
        nsworkspace_icon_for_file(&app.path.to_string_lossy())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================
    // Bundle fixtures
    //
    // Each test builds bundles in a temporary directory. Its path
    // lies under /var/folders, which has no `Applications` folder,
    // so a fixture passes the folder check only when the test puts
    // it below an `Applications` folder of its own.
    // =========================================================

    /// Create `<root>/<relative>/Contents/Info.plist` holding `info`
    /// and return the bundle path.
    fn write_bundle(root: &Path, relative: &str, info: plist::Dictionary) -> PathBuf {
        let bundle = root.join(relative);
        let contents = bundle.join("Contents");
        std::fs::create_dir_all(&contents).expect("temporary directory is writable");
        plist::to_file_xml(contents.join("Info.plist"), &plist::Value::Dictionary(info))
            .expect("temporary directory is writable");
        bundle
    }

    fn plist_with_name(name: &str) -> plist::Dictionary {
        let mut info = plist::Dictionary::new();
        info.insert("CFBundleName".into(), name.into());
        info
    }

    fn names(apps: &[DiscoveredApp]) -> Vec<&str> {
        apps.iter().map(|app| app.name.as_str()).collect()
    }

    #[test]
    fn a_top_level_app_is_listed_with_its_plist_metadata() {
        let root = tempfile::tempdir().expect("temporary directory can be created");
        let mut info = plist_with_name("Safari");
        info.insert("CFBundleDisplayName".into(), "Safari Display".into());
        info.insert("CFBundleIdentifier".into(), "com.apple.Safari".into());
        let bundle = write_bundle(root.path(), "Applications/Safari.app", info);

        let apps = apps_from_bundle_paths(std::slice::from_ref(&bundle));

        assert_eq!(apps.len(), 1);
        let app = &apps[0];
        assert_eq!(app.id, bundle.to_string_lossy());
        assert_eq!(app.name, "Safari Display");
        assert_eq!(app.path, bundle);
        assert_eq!(app.bundle_id.as_deref(), Some("com.apple.Safari"));
        assert_eq!(app.icon_path, None);
    }

    #[test]
    fn the_name_falls_back_to_cfbundlename() {
        let root = tempfile::tempdir().expect("temporary directory can be created");
        let bundle = write_bundle(
            root.path(),
            "Applications/Notes.app",
            plist_with_name("Notes Name"),
        );

        let apps = apps_from_bundle_paths(&[bundle]);

        assert_eq!(names(&apps), ["Notes Name"]);
        assert_eq!(apps[0].bundle_id, None);
    }

    #[test]
    fn the_name_falls_back_to_the_file_stem() {
        let root = tempfile::tempdir().expect("temporary directory can be created");
        let bundle = write_bundle(
            root.path(),
            "Applications/Plain Tool.app",
            plist::Dictionary::new(),
        );

        let apps = apps_from_bundle_paths(&[bundle]);

        assert_eq!(names(&apps), ["Plain Tool"]);
    }

    #[test]
    fn a_bundle_outside_an_applications_folder_is_skipped() {
        let root = tempfile::tempdir().expect("temporary directory can be created");
        let bundle = write_bundle(root.path(), "Downloads/Stray.app", plist_with_name("Stray"));

        let apps = apps_from_bundle_paths(&[bundle]);

        assert!(apps.is_empty(), "listed: {:?}", names(&apps));
    }

    #[test]
    fn a_bundle_without_a_readable_plist_is_skipped() {
        let root = tempfile::tempdir().expect("temporary directory can be created");
        let broken = root.path().join("Applications/Broken.app");
        std::fs::create_dir_all(&broken).expect("temporary directory is writable");
        let valid = write_bundle(
            root.path(),
            "Applications/Valid.app",
            plist_with_name("Valid"),
        );

        let apps = apps_from_bundle_paths(&[broken, valid]);

        assert_eq!(names(&apps), ["Valid"]);
    }

    #[test]
    fn menu_bar_and_background_apps_stay_listed() {
        let root = tempfile::tempdir().expect("temporary directory can be created");
        let mut menu_bar = plist_with_name("Menu Bar");
        menu_bar.insert("LSUIElement".into(), true.into());
        let mut menu_bar_string = plist_with_name("Menu Bar String");
        menu_bar_string.insert("LSUIElement".into(), "1".into());
        let mut background = plist_with_name("Background");
        background.insert("LSBackgroundOnly".into(), true.into());
        let bundles = [
            write_bundle(root.path(), "Applications/Menu Bar.app", menu_bar),
            write_bundle(
                root.path(),
                "Applications/Menu Bar String.app",
                menu_bar_string,
            ),
            write_bundle(root.path(), "Applications/Background.app", background),
        ];

        let apps = apps_from_bundle_paths(&bundles);

        assert_eq!(names(&apps), ["Menu Bar", "Menu Bar String", "Background"]);
    }

    #[test]
    fn an_app_in_contents_applications_of_a_bundle_outside_applications_stays_listed() {
        let root = tempfile::tempdir().expect("temporary directory can be created");
        let finder = write_bundle(
            root.path(),
            "CoreServices/Finder.app",
            plist_with_name("Finder"),
        );
        let airdrop = write_bundle(
            &finder,
            "Contents/Applications/AirDrop.app",
            plist_with_name("AirDrop"),
        );

        let apps = apps_from_bundle_paths(&[finder, airdrop]);

        assert_eq!(names(&apps), ["AirDrop"]);
    }

    // =========================================================
    // Bundles nested in other apps
    // =========================================================

    /// Build a chain of nested bundles, each path relative to the
    /// bundle before it (the first relative to a fresh temporary
    /// directory), name each after its file stem, and return the
    /// names `apps_from_bundle_paths` lists.
    fn listed_names_of_chain(chain: &[&str]) -> Vec<String> {
        let root = tempfile::tempdir().expect("temporary directory can be created");
        let mut parent = root.path().to_owned();
        let mut bundles = Vec::new();
        for relative in chain {
            let stem = Path::new(relative)
                .file_stem()
                .expect("chain entries name a bundle")
                .to_string_lossy()
                .into_owned();
            let bundle = write_bundle(&parent, relative, plist_with_name(&stem));
            bundles.push(bundle.clone());
            parent = bundle;
        }

        apps_from_bundle_paths(&bundles)
            .into_iter()
            .map(|app| app.name)
            .collect()
    }

    #[test]
    fn a_login_item_inside_an_app_is_skipped() {
        let listed = listed_names_of_chain(&[
            "Applications/Foo.app",
            "Contents/Library/LoginItems/Foo Login.app",
        ]);

        assert_eq!(listed, ["Foo"], "helper listed");
    }

    #[test]
    fn a_helper_inside_a_framework_in_an_app_is_skipped() {
        let listed = listed_names_of_chain(&[
            "Applications/Foo.app",
            "Contents/Frameworks/Foo Framework.framework/Versions/A/Helpers/Foo Helper.app",
        ]);

        assert_eq!(listed, ["Foo"], "helper listed");
    }

    #[test]
    fn an_app_in_a_subfolder_of_contents_applications_is_skipped() {
        let listed = listed_names_of_chain(&[
            "Applications/Foo.app",
            "Contents/Applications/Extras/Bar.app",
        ]);

        assert_eq!(listed, ["Foo"], "helper listed");
    }

    #[test]
    fn an_app_inside_a_helper_is_skipped() {
        let listed = listed_names_of_chain(&[
            "Applications/Foo.app",
            "Contents/Library/LoginItems/Bar.app",
            "Contents/Applications/Baz.app",
        ]);

        assert_eq!(listed, ["Foo"], "helper listed");
    }

    #[test]
    fn an_app_in_contents_applications_stays_listed() {
        let listed = listed_names_of_chain(&[
            "Applications/Xcode.app",
            "Contents/Applications/Instruments.app",
        ]);

        assert_eq!(listed, ["Xcode", "Instruments"]);
    }

    #[test]
    fn an_app_in_contents_developer_applications_stays_listed() {
        let listed = listed_names_of_chain(&[
            "Applications/Xcode.app",
            "Contents/Developer/Applications/Simulator.app",
        ]);

        assert_eq!(listed, ["Xcode", "Simulator"]);
    }

    #[test]
    fn an_app_nested_through_two_applications_folders_stays_listed() {
        let listed = listed_names_of_chain(&[
            "Applications/Outer.app",
            "Contents/Applications/Middle.app",
            "Contents/Applications/Inner.app",
        ]);

        assert_eq!(listed, ["Outer", "Middle", "Inner"]);
    }

    #[test]
    fn a_top_level_app_is_no_embedded_helper() {
        assert!(!is_embedded_helper(Path::new("/Applications/Safari.app")));
    }

    #[test]
    fn a_folder_merely_named_like_an_app_is_no_host_bundle() {
        assert!(!is_embedded_helper(Path::new(
            "/Applications/Old.app.backup/Bar.app"
        )));
    }

    #[test]
    fn an_app_directly_in_contents_is_an_embedded_helper() {
        assert!(is_embedded_helper(Path::new(
            "/Applications/Foo.app/Contents/Bar.app"
        )));
    }

    #[test]
    fn an_app_directly_in_an_applications_folder_of_an_app_is_no_embedded_helper() {
        assert!(!is_embedded_helper(Path::new(
            "/Applications/Foo.app/Applications/Bar.app"
        )));
    }

    #[test]
    fn an_app_below_a_helper_is_an_embedded_helper() {
        assert!(is_embedded_helper(Path::new(
            "/Applications/Foo.app/Contents/Library/LoginItems/Bar.app/Contents/Applications/Baz.app"
        )));
    }

    #[test]
    fn a_helper_with_a_non_utf8_name_is_an_embedded_helper() {
        use std::os::unix::ffi::OsStrExt;

        let path = Path::new("/Applications/Foo.app/Contents/Library/LoginItems")
            .join(OsStr::from_bytes(b"\xff.app"));

        assert!(is_embedded_helper(&path));
    }

    // =========================================================
    // Discovery that ends with no application
    // =========================================================

    /// The error of a discovery that must have failed.
    fn error_of(result: anyhow::Result<Vec<DiscoveredApp>>) -> anyhow::Error {
        match result {
            Ok(apps) => panic!("discovery succeeded with {:?}", names(&apps)),
            Err(error) => error,
        }
    }

    #[test]
    fn no_spotlight_output_is_an_error() {
        let result = discover_from(|| Ok(Vec::new()));

        let error = error_of(result);
        assert!(
            format!("{error:#}").contains("of 0 Spotlight results"),
            "message: {error:#}"
        );
    }

    #[test]
    fn only_paths_outside_applications_folders_is_an_error() {
        let root = tempfile::tempdir().expect("temporary directory can be created");
        let stray = write_bundle(root.path(), "Downloads/Stray.app", plist_with_name("Stray"));

        let result = discover_from(|| Ok(vec![stray]));

        error_of(result);
    }

    #[test]
    fn only_embedded_helpers_is_an_error() {
        let helper =
            PathBuf::from("/Applications/Foo.app/Contents/Library/LoginItems/Foo Login.app");

        let result = discover_from(|| Ok(vec![helper]));

        error_of(result);
    }

    #[test]
    fn only_unreadable_bundles_is_an_error() {
        let root = tempfile::tempdir().expect("temporary directory can be created");
        let broken = root.path().join("Applications/Broken.app");
        std::fs::create_dir_all(&broken).expect("temporary directory is writable");

        let result = discover_from(|| Ok(vec![broken]));

        error_of(result);
    }

    #[test]
    fn the_error_counts_the_dropped_results() {
        let paths = vec![
            PathBuf::from("/Users/someone/Downloads/Stray.app"),
            PathBuf::from("/Applications/Foo.app/Contents/Library/LoginItems/Foo Login.app"),
        ];

        let result = discover_from(|| Ok(paths));

        let error = error_of(result);
        assert!(
            format!("{error:#}").contains("no application bundle left of 2 Spotlight results"),
            "message: {error:#}"
        );
    }

    #[test]
    fn a_failing_query_keeps_its_context() {
        let result = discover_from(|| Err(anyhow::anyhow!("mdfind exited with 1")));

        let error = error_of(result);
        let chain = format!("{error:#}");
        assert!(
            chain.contains("query Spotlight for application bundles"),
            "message: {chain}"
        );
        assert!(chain.contains("mdfind exited with 1"), "message: {chain}");
    }

    #[test]
    fn one_valid_app_among_dropped_paths_is_a_success() {
        let root = tempfile::tempdir().expect("temporary directory can be created");
        let valid = write_bundle(
            root.path(),
            "Applications/Valid.app",
            plist_with_name("Valid"),
        );
        let stray = PathBuf::from("/Users/someone/Downloads/Stray.app");

        let apps = discover_from(|| Ok(vec![stray, valid])).expect("one app is left");

        assert_eq!(names(&apps), ["Valid"]);
    }

    // =========================================================
    // Time-limited commands
    //
    // These tests run real short-lived processes. Where a test
    // depends on wall time, the limit and the process length differ
    // by a factor of at least 25.
    // =========================================================

    fn sh(script: &str) -> Command {
        let mut command = Command::new("sh");
        command.args(["-c", script]);
        command
    }

    #[test]
    fn a_run_inside_the_limit_returns_its_output() {
        let output = output_within(&mut sh("sleep 0.2; printf ok"), Duration::from_secs(10))
            .expect("the run ends inside the limit");

        assert!(output.status.success());
        assert_eq!(output.stdout, b"ok");
    }

    #[test]
    fn a_failing_status_and_stderr_are_returned() {
        let output = output_within(&mut sh("echo oops >&2; exit 3"), Duration::from_secs(10))
            .expect("a failing exit is not an error here");

        assert_eq!(output.status.code(), Some(3));
        assert_eq!(output.stderr, b"oops\n");
    }

    #[test]
    fn a_missing_program_is_an_error_with_context() {
        let result = output_within(
            &mut Command::new("torchsnap-test-no-such-tool"),
            Duration::from_secs(10),
        );

        let error = result.expect_err("the program does not exist");
        assert!(
            format!("{error:#}").contains("spawn torchsnap-test-no-such-tool process"),
            "message: {error:#}"
        );
    }

    #[test]
    fn output_larger_than_the_pipe_buffer_arrives_whole() {
        let output = output_within(
            Command::new("head").args(["-c", "1048576", "/dev/zero"]),
            Duration::from_secs(10),
        )
        .expect("the run ends inside the limit");

        assert_eq!(output.stdout.len(), 1_048_576);
    }

    #[test]
    fn stderr_larger_than_the_pipe_buffer_does_not_block_the_run() {
        let output = output_within(
            &mut sh("head -c 1048576 /dev/zero >&2; head -c 1048576 /dev/zero"),
            Duration::from_secs(10),
        )
        .expect("the run ends inside the limit");

        assert_eq!(output.stderr.len(), 1_048_576);
        assert_eq!(output.stdout.len(), 1_048_576);
    }

    #[test]
    fn a_run_past_the_limit_is_an_error() {
        let result = output_within(&mut sh("sleep 5"), Duration::from_millis(100));

        let error = result.expect_err("the run outlasts the limit");
        assert!(
            format!("{error:#}").contains("did not finish within"),
            "message: {error:#}"
        );
    }

    #[test]
    fn a_run_past_the_limit_is_killed_and_reaped() {
        let root = tempfile::tempdir().expect("temporary directory can be created");
        let pid_file = root.path().join("pid");
        let mut command = sh(r#"echo $$ > "$1"; exec sleep 5"#);
        command.arg("sh").arg(&pid_file);

        let started = Instant::now();
        let result = output_within(&mut command, Duration::from_secs(2));
        let elapsed = started.elapsed();

        assert!(result.is_err(), "the run outlasts the limit");
        let pid: i32 = std::fs::read_to_string(&pid_file)
            .ok()
            .and_then(|text| text.trim().parse().ok())
            .expect("child never wrote its pid");
        // A zombie still answers signal 0, so ESRCH proves the reap.
        // SAFETY: signal 0 only checks that the process exists.
        let alive = unsafe { libc::kill(pid, 0) } == 0;
        assert!(!alive, "child {pid} is still there");
        assert!(
            elapsed < Duration::from_secs(4),
            "returned after {elapsed:?}, so the child was waited for, not killed"
        );
    }

    #[test]
    fn a_pipe_held_open_after_exit_is_an_error() {
        let started = Instant::now();
        let result = output_within(&mut sh("sleep 5 & exit 0"), Duration::from_millis(200));

        assert!(result.is_err(), "output came back with a pipe still open");
        assert!(started.elapsed() < Duration::from_secs(4));
    }

    #[test]
    fn query_lines_become_paths_and_empty_lines_are_dropped() {
        let paths = query_command(
            &mut sh(r"printf '/Applications/A.app\n\n/Applications/B.app\n'"),
            Duration::from_secs(10),
        )
        .expect("the command succeeds");

        assert_eq!(
            paths,
            [
                PathBuf::from("/Applications/A.app"),
                PathBuf::from("/Applications/B.app")
            ]
        );
    }

    #[test]
    fn a_failing_query_carries_its_stderr() {
        let error = query_command(
            &mut sh("echo no index >&2; exit 1"),
            Duration::from_secs(10),
        )
        .expect_err("the command fails");

        let chain = format!("{error:#}");
        assert!(chain.contains("sh exited with"), "message: {chain}");
        assert!(chain.contains("no index"), "message: {chain}");
    }

    #[test]
    fn query_output_that_is_not_utf8_is_an_error() {
        let error = query_command(&mut sh(r"printf '\377'"), Duration::from_secs(10))
            .expect_err("0xff is not UTF-8");

        assert!(
            format!("{error:#}").contains("decode sh output as UTF-8"),
            "message: {error:#}"
        );
    }

    #[test]
    fn a_query_past_the_limit_is_an_error() {
        let error = query_command(&mut sh("sleep 5"), Duration::from_millis(100))
            .expect_err("the run outlasts the limit");

        assert!(
            format!("{error:#}").contains("did not finish within"),
            "message: {error:#}"
        );
    }
}
