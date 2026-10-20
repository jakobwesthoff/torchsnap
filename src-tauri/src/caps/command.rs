// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// CommandCap
//
// Process-spawning capability with permission checking, a
// minimal environment, timeout enforcement, and output
// capping. The gadget declares `[[permissions.command]]`
// rules in its manifest; the bridge compiles them via
// `argv_matcher` and passes them here. At runtime, `run()`
// matches the requested `(binary, argv)` against the compiled
// rules — no match means `PermissionDenied`.
//
// All I/O (spawn, stdin write, stdout/stderr drain) happens
// inside `tokio::task::block_in_place` so the synchronous-
// from-the-guest's-perspective call doesn't block the tokio
// executor.
// =========================================================

use std::path::PathBuf;

use crate::paths::PathResolver;
use crate::wasm::argv_matcher::{self, CompiledCommandRule};
use crate::wasm::manifest::CommandPermissionDef;
use crate::wasm::manifest::permissions::path_tokens;
use crate::wasm::path_safety::{Containment, lies_within};

// =========================================================
// Constants
// =========================================================

const DEFAULT_COMMAND_TIMEOUT_MS: u64 = 10_000;
const MAX_COMMAND_TIMEOUT_MS: u64 = 60_000;
const DEFAULT_COMMAND_OUTPUT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_COMMAND_OUTPUT_BYTES: u64 = 64 * 1024 * 1024;

/// Resolve the effective command timeout from the gadget-requested
/// value: default when unset, clamped to the host maximum.
fn resolve_timeout_ms(requested: Option<u32>) -> u64 {
    requested
        .map(|v| v as u64)
        .unwrap_or(DEFAULT_COMMAND_TIMEOUT_MS)
        .min(MAX_COMMAND_TIMEOUT_MS)
}

/// Resolve the effective output-size cap from the gadget-requested
/// value: default when unset, clamped to the host maximum.
fn resolve_max_output_bytes(requested: Option<u64>) -> u64 {
    requested
        .unwrap_or(DEFAULT_COMMAND_OUTPUT_BYTES)
        .min(MAX_COMMAND_OUTPUT_BYTES)
}

// =========================================================
// Error type
// =========================================================

/// Command capability error. Mirrors the WIT `command-error`
/// variants 1:1; the WASM bridge layer provides `From` impls
/// for the WIT type.
#[derive(Debug, thiserror::Error)]
pub enum CommandCapError {
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("spawn failed: {0}")]
    SpawnFailed(String),
    #[error("command timed out")]
    Timeout,
    #[error("output too large")]
    OutputTooLarge(Vec<u8>, Vec<u8>),
}

// =========================================================
// Native types
// =========================================================

/// Options for a single `command::run` call. Fields mirror
/// the WIT `command-options` record; the bridge provides
/// `From` impls for conversion.
pub struct CommandOptions {
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env: Vec<(String, String)>,
    pub stdin: Option<Vec<u8>>,
    pub timeout_ms: Option<u32>,
    pub max_output_bytes: Option<u64>,
}

/// Result of a successful `command::run` call. Maps 1:1 to
/// the WIT `command-result` record.
#[derive(Debug)]
pub struct CommandResult {
    pub exit_code: Option<i32>,
    pub signal: Option<String>,
    pub timed_out: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

// =========================================================
// CommandPermissions
// =========================================================

/// Permission configuration for constructing a `CommandCap`.
/// Wraps the raw manifest command rules; a domain-specific
/// type replacing the manifest struct is tracked for a later
/// todo.
pub struct CommandPermissions {
    pub rules: Vec<CommandPermissionDef>,
}

// =========================================================
// CommandCap
// =========================================================

pub struct CommandCap {
    rules: Vec<CompiledCommandRule>,
    writable: WritableAreas,
    /// Where each spawn's private directories are created (see
    /// [`SpawnDirs`]).
    spawn_base: PathBuf,
}

/// Compile `rules_raw` and check that no root or fixed value of them
/// lies in an area the gadget can write. The install review runs this
/// so a gadget that would fail to load is refused before it is
/// installed; [`CommandCap::new`] runs it again at load.
pub fn check_command_rules(
    rules_raw: &[CommandPermissionDef],
    resolver: &impl PathResolver,
    gadget_data_dir: &std::path::Path,
) -> anyhow::Result<()> {
    let writable = WritableAreas::for_gadget(gadget_data_dir, resolver)?;
    compile_checked_rules(rules_raw, resolver, &writable).map(|_| ())
}

fn compile_checked_rules(
    rules_raw: &[CommandPermissionDef],
    resolver: &impl PathResolver,
    writable: &WritableAreas,
) -> anyhow::Result<Vec<CompiledCommandRule>> {
    let mut rules = Vec::with_capacity(rules_raw.len());
    for (index, raw) in rules_raw.iter().enumerate() {
        let rule = argv_matcher::compile_rule(raw, index, resolver)?;
        check_rule_locations(&rule, index, writable)?;
        rules.push(rule);
    }
    Ok(rules)
}

impl CommandCap {
    /// Compile raw `[[permissions.command]]` rules from the
    /// manifest against the provided resolver and construct
    /// a capability. Fails on the first rule that cannot be
    /// compiled (consistent with `FilesystemCap::new`), or whose
    /// roots or fixed values lie in an area the gadget can write.
    pub fn new(
        rules_raw: &[CommandPermissionDef],
        resolver: &impl PathResolver,
        gadget_data_dir: PathBuf,
    ) -> anyhow::Result<Self> {
        let writable = WritableAreas::for_gadget(&gadget_data_dir, resolver)?;
        let rules = compile_checked_rules(rules_raw, resolver, &writable)?;
        let spawn_base = default_spawn_base();
        sweep_stale_spawn_dirs(&spawn_base, STALE_SPAWN_DIR_AGE);
        Ok(Self {
            rules,
            writable,
            spawn_base,
        })
    }

    /// Construct from pre-compiled rules. Test-only escape
    /// hatch so integration tests can build fixtures with
    /// hand-crafted rule sets without going through the full
    /// manifest → compile pipeline. `app_data_dir` is taken as the
    /// gadget's writable area as it is.
    #[cfg(test)]
    pub(crate) fn from_compiled_rules(
        rules: Vec<CompiledCommandRule>,
        app_data_dir: PathBuf,
    ) -> Self {
        Self {
            rules,
            writable: WritableAreas {
                app_data: app_data_dir,
                package: None,
            },
            spawn_base: default_spawn_base(),
        }
    }

    /// Create spawn directories below `base` instead of the host's
    /// temp directory, so a test can see what a run leaves behind.
    #[cfg(test)]
    pub(crate) fn with_spawn_base(mut self, base: PathBuf) -> Self {
        self.spawn_base = base;
        self
    }

    /// Execute a command if the `(binary, argv)` pair matches
    /// a compiled permission rule. Handles cwd resolution,
    /// environment filtering, process spawn, timeout
    /// enforcement, and output capping.
    pub fn run(
        &self,
        binary: &str,
        options: CommandOptions,
    ) -> Result<CommandResult, CommandCapError> {
        let Ok(rule) = argv_matcher::matches(&self.rules, binary, &options.args) else {
            return Err(CommandCapError::PermissionDenied(format!(
                "no `[[permissions.command]]` rule accepts `{binary}` with the given argv"
            )));
        };

        // Every check that can deny the call runs before anything is
        // created on disk.
        for value in argv_matcher::path_under_arguments(rule, &options.args) {
            self.writable
                .deny_touching(std::path::Path::new(value), Scope::Argument)
                .map_err(|reason| {
                    CommandCapError::PermissionDenied(format!("argument \"{value}\" {reason}"))
                })?;
        }
        let env_overrides = check_env_overrides(rule, &options.env, &self.writable)?;
        let guest_cwd = check_guest_cwd(rule, options.cwd.as_deref(), &self.writable)?;
        let rule_default_cwd = match (&guest_cwd, &rule.cwd) {
            (None, Some(root)) => Some(prepare_rule_root(root, &self.writable)?),
            _ => None,
        };

        // Lives until `run` returns, after the child has exited or been
        // killed, so the directories outlast every use by the child.
        let spawn_dirs = SpawnDirs::create(&self.spawn_base).map_err(|e| {
            CommandCapError::SpawnFailed(format!(
                "create the spawn directory in \"{}\": {e}",
                self.spawn_base.display()
            ))
        })?;

        let cwd = guest_cwd
            .or(rule_default_cwd)
            .unwrap_or_else(|| spawn_dirs.cwd().to_path_buf());

        let env = build_command_env(std::env::vars_os(), &spawn_dirs, &env_overrides);

        let timeout_ms = resolve_timeout_ms(options.timeout_ms);
        let max_output_bytes = resolve_max_output_bytes(options.max_output_bytes);

        tokio::task::block_in_place(|| {
            let runtime_handle = tokio::runtime::Handle::current();
            runtime_handle.block_on(run_child_with_caps(
                binary,
                &options.args,
                &cwd,
                env,
                options.stdin,
                timeout_ms,
                max_output_bytes,
            ))
        })
    }
}

// =========================================================
// The gadget's writable areas
//
// Nothing the gadget can write may be visible to a program as
// configuration. Every root a rule names, every fixed value that
// names a path, and every path a call passes is kept out of the
// areas whose files the gadget controls. A working directory or a
// path argument also may not contain such an area, since programs
// look for configuration below the directories they are given.
// =========================================================

/// The areas whose files the gadget controls.
///
/// macOS-specific in what they hold: `app_data` is the app's data
/// directory with every gadget's `gadget-home/<id>/`, the installed
/// packages in `gadgets/`, and the host's own stores, which keep
/// values gadgets set. Other platforms need their own choice (todo
/// `01M5DN71VBN0H0AVSQT2GMDK49-decide-how-command-confinement-works-on-linux-and-windows.md`).
#[derive(Debug, Clone)]
struct WritableAreas {
    app_data: PathBuf,
    /// The gadget's own package (`${gadget-archive}`). A directory for
    /// a development gadget, the archive file for an installed one.
    package: Option<PathBuf>,
}

/// What a path is used for, which decides the areas it must avoid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    /// A working directory or an environment value: everything the
    /// gadget controls, its own package included.
    Configuration,
    /// A program argument: the app data, except the gadget's own
    /// package, whose scripts a granted interpreter may run. The review
    /// shows such an argument as written.
    Argument,
}

impl WritableAreas {
    /// The areas of the gadget whose data directory is
    /// `gadget_data_dir`, which the host lays out as
    /// `<app data>/gadget-home/<id>/`.
    fn for_gadget(
        gadget_data_dir: &std::path::Path,
        resolver: &impl PathResolver,
    ) -> anyhow::Result<Self> {
        let gadget_home = gadget_data_dir
            .parent()
            .filter(|parent| parent.file_name().is_some_and(|name| name == "gadget-home"));
        let Some(app_data) = gadget_home.and_then(std::path::Path::parent) else {
            anyhow::bail!(
                "the gadget's data directory \"{}\" is not in the host's layout \
                 <app data>/gadget-home/<id>/",
                gadget_data_dir.display()
            );
        };
        let package = resolver
            .resolve("gadget-archive")
            .filter(|package| !package.as_os_str().is_empty())
            .map(std::path::Path::to_path_buf);
        Ok(Self {
            app_data: app_data.to_path_buf(),
            package,
        })
    }

    /// Whether `path` lies in an area of `scope`.
    fn holds(&self, path: &std::path::Path, scope: Scope) -> Containment {
        let in_package = || match &self.package {
            Some(package) => lies_within(path, package),
            None => Containment::Outside,
        };
        match scope {
            Scope::Configuration => any_inside([lies_within(path, &self.app_data), in_package()]),
            Scope::Argument => match lies_within(path, &self.app_data) {
                Containment::Inside if in_package() == Containment::Inside => Containment::Outside,
                other => other,
            },
        }
    }

    /// Whether `path` lies in an area of `scope` or contains one.
    fn touches(&self, path: &std::path::Path, scope: Scope) -> Containment {
        let contains_package = match (&self.package, scope) {
            (Some(package), Scope::Configuration) => lies_within(package, path),
            _ => Containment::Outside,
        };
        any_inside([
            self.holds(path, scope),
            lies_within(&self.app_data, path),
            contains_package,
        ])
    }

    /// `Err` with the reason when `path` lies in or contains an area of
    /// `scope`, or when that cannot be established.
    fn deny_touching(&self, path: &std::path::Path, scope: Scope) -> Result<(), String> {
        match self.touches(path, scope) {
            Containment::Outside => Ok(()),
            Containment::Inside => Err(format!(
                "lies in or contains {}, which the gadget can write",
                self.describe_area_near(path, scope)
            )),
            Containment::Undetermined(reason) => Err(format!(
                "cannot be checked against the areas the gadget can write: {reason}"
            )),
        }
    }

    /// The area `path` lies in or contains, in words: the gadget's
    /// package when it is that one, else the app data directory.
    fn describe_area_near(&self, path: &std::path::Path, scope: Scope) -> String {
        match &self.package {
            Some(package)
                if scope == Scope::Configuration
                    && (lies_within(path, package) == Containment::Inside
                        || lies_within(package, path) == Containment::Inside) =>
            {
                format!("the gadget's package \"{}\"", package.display())
            }
            _ => format!("Torchsnap's data folder \"{}\"", self.app_data.display()),
        }
    }
}

/// `Inside` if any is, else the first `Undetermined`, else `Outside`.
fn any_inside(results: impl IntoIterator<Item = Containment>) -> Containment {
    let mut undetermined = None;
    for result in results {
        match result {
            Containment::Inside => return Containment::Inside,
            Containment::Undetermined(reason) => {
                undetermined.get_or_insert(reason);
            }
            Containment::Outside => {}
        }
    }
    undetermined.map_or(Containment::Outside, Containment::Undetermined)
}

/// Load-time check of one compiled rule: its `cwd` root, its
/// `path-under` roots and the paths in its fixed values must not lie in
/// an area the gadget can write. A path that cannot be resolved now (a
/// volume not mounted, a dangling link) is accepted here; the check at
/// run time denies it if it is still unresolved then.
fn check_rule_locations(
    rule: &CompiledCommandRule,
    rule_index: usize,
    writable: &WritableAreas,
) -> anyhow::Result<()> {
    use argv_matcher::CompiledArgvConstraint as C;

    let check = |path: &std::path::Path, scope: Scope, field: &str| -> anyhow::Result<()> {
        match writable.holds(path, scope) {
            Containment::Inside => anyhow::bail!(
                "Command rule {rule_index}: {field} \"{}\" lies in {}, which the gadget can write",
                path.display(),
                writable.describe_area_near(path, scope)
            ),
            Containment::Undetermined(reason) => {
                eprintln!(
                    "Command rule {rule_index}: {field} \"{}\" is checked when a call uses it, \
                     since it cannot be resolved now: {reason}",
                    path.display()
                );
                Ok(())
            }
            Containment::Outside => Ok(()),
        }
    };
    let check_constraint = |constraint: &C, scope: Scope, field: &str| -> anyhow::Result<()> {
        let constraint = match constraint {
            C::Rest(inner) => inner.as_ref(),
            other => other,
        };
        match constraint {
            C::PathUnder(root) => check(root, scope, &format!("the path-under root of {field}")),
            C::Literal(value) => check_fixed_value(value, scope, field, &check),
            C::Enum(values) => values
                .iter()
                .try_for_each(|value| check_fixed_value(value, scope, field, &check)),
            C::Glob(_) | C::Regex(_) | C::AnyString | C::Rest(_) => Ok(()),
        }
    };

    if let Some(root) = &rule.cwd {
        check(root, Scope::Configuration, "the working directory")?;
    }
    for (argv_index, constraint) in rule.argv.iter().enumerate() {
        check_constraint(
            constraint,
            Scope::Argument,
            &format!("argument {argv_index}"),
        )?;
    }
    for (key, constraint) in &rule.env {
        check_constraint(
            constraint,
            Scope::Configuration,
            &format!("the environment variable {key}"),
        )?;
    }
    Ok(())
}

/// Check every absolute path inside a fixed value, alone, in a list or
/// after `--option=`.
fn check_fixed_value(
    value: &str,
    scope: Scope,
    field: &str,
    check: &impl Fn(&std::path::Path, Scope, &str) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    path_tokens(value)
        .into_iter()
        .filter(|token| token.starts_with('/'))
        .try_for_each(|token| check(std::path::Path::new(token), scope, field))
}

// =========================================================
// Working directory
//
// A rule's `cwd` is the root of the working directories a call
// may choose; without one, a call cannot choose at all. The child
// is started in the canonical path, so what was checked is what
// it runs in. A guest path with `..` is refused outright: the
// lexical check reads `root/link/..` as `root`, while the kernel
// resolves it against the link's target.
// =========================================================

/// The working directory the guest asked for, checked against the
/// rule's root, or `None` when it asked for none.
fn check_guest_cwd(
    rule: &CompiledCommandRule,
    guest: Option<&str>,
    writable: &WritableAreas,
) -> Result<Option<PathBuf>, CommandCapError> {
    use crate::wasm::path_safety::{self, PathError};

    let Some(guest) = guest else {
        return Ok(None);
    };
    let Some(root) = &rule.cwd else {
        return Err(CommandCapError::PermissionDenied(format!(
            "the rule for \"{}\" declares no cwd, so a call cannot choose a working directory",
            rule.binary
        )));
    };
    let requested = std::path::Path::new(guest);
    if requested
        .components()
        .any(|component| component == std::path::Component::ParentDir)
    {
        return Err(CommandCapError::PermissionDenied(format!(
            "working directory \"{guest}\" contains \"..\""
        )));
    }

    let canonical = match path_safety::canonical_under_root(requested, root) {
        Ok(canonical) => canonical,
        Err(
            e @ (PathError::CandidateNotAbsolute(_)
            | PathError::DanglingSymlink(_)
            | PathError::EscapesRoot { .. }),
        ) => {
            return Err(CommandCapError::PermissionDenied(format!(
                "working directory \"{guest}\" is not inside the rule's cwd: {e}"
            )));
        }
        Err(
            e @ (PathError::RootNotAbsolute(_)
            | PathError::RootCanonicalize { .. }
            | PathError::CandidateCanonicalize(_)),
        ) => {
            return Err(CommandCapError::SpawnFailed(format!(
                "resolve working directory \"{guest}\": {e}"
            )));
        }
    };

    writable
        .deny_touching(&canonical, Scope::Configuration)
        .map_err(|reason| {
            CommandCapError::PermissionDenied(format!("working directory \"{guest}\" {reason}"))
        })?;

    match std::fs::metadata(&canonical) {
        Ok(meta) if meta.is_dir() => Ok(Some(canonical)),
        Ok(_) => Err(CommandCapError::SpawnFailed(format!(
            "working directory \"{}\" is not a directory",
            canonical.display()
        ))),
        Err(e) => Err(CommandCapError::SpawnFailed(format!(
            "working directory \"{}\": {e}",
            canonical.display()
        ))),
    }
}

/// The rule's root as the default working directory, created when
/// it does not exist yet. It is checked before it is created, since
/// creating follows symlinks, and again once it is canonical.
fn prepare_rule_root(
    root: &std::path::Path,
    writable: &WritableAreas,
) -> Result<PathBuf, CommandCapError> {
    let deny = |reason: String| {
        CommandCapError::PermissionDenied(format!("the rule's cwd \"{}\" {reason}", root.display()))
    };
    writable
        .deny_touching(root, Scope::Configuration)
        .map_err(deny)?;
    let canonical = std::fs::create_dir_all(root)
        .and_then(|()| root.canonicalize())
        .map_err(|e| {
            CommandCapError::SpawnFailed(format!(
                "prepare the rule's cwd \"{}\": {e}",
                root.display()
            ))
        })?;
    writable
        .deny_touching(&canonical, Scope::Configuration)
        .map_err(deny)?;
    Ok(canonical)
}

// =========================================================
// Per-spawn directories
//
// Every spawn gets a fresh, empty directory tree that nothing
// else has written to. A program run without a rule `cwd` works
// in its `cwd/`, so no file the gadget planted earlier can act as
// its configuration. The tree is removed when the run ends.
//
// macOS-specific: the tree lives in `std::env::temp_dir()`, which
// is the per-user `$TMPDIR` below `/var/folders/` there; on Linux
// it is the shared `/tmp`. The 0700 mode and the chmod-then-remove
// cleanup rely on Unix permissions. Other platforms need their
// own choice (todo
// `01M5DN71VBN0H0AVSQT2GMDK49-decide-how-command-confinement-works-on-linux-and-windows.md`).
// =========================================================

const SPAWN_DIR_PREFIX: &str = "torchsnap-command-";

fn default_spawn_base() -> PathBuf {
    std::env::temp_dir()
}

/// Spawn directories older than this are left over from a host that
/// exited without removing them; a call lasts at most a minute.
const STALE_SPAWN_DIR_AGE: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// Remove spawn directories in `base` that a crashed or killed host
/// left behind: entries with the spawn prefix that are directories
/// (not symlinks), owned by this user, and older than `max_age`.
/// Best effort; an entry that cannot be inspected or removed stays.
fn sweep_stale_spawn_dirs(base: &std::path::Path, max_age: std::time::Duration) {
    let Ok(entries) = std::fs::read_dir(base) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with(SPAWN_DIR_PREFIX)
        {
            continue;
        }
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        #[cfg(unix)]
        let owned_by_us = {
            use std::os::unix::fs::MetadataExt;
            // SAFETY: `getuid` has no preconditions and cannot fail.
            meta.uid() == unsafe { libc::getuid() }
        };
        #[cfg(not(unix))]
        let owned_by_us = true;
        let stale = meta
            .modified()
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age > max_age);
        if meta.is_dir() && owned_by_us && stale {
            make_tree_removable(&path);
            let _ = std::fs::remove_dir_all(&path);
        }
    }
}

/// The directories of one spawn, removed on drop: the working
/// directory for a rule without `cwd`, and the child's `HOME` and
/// `TMPDIR`, kept apart so the program's temporary files do not
/// land where it looks for configuration.
struct SpawnDirs {
    root: PathBuf,
    cwd: PathBuf,
    home: PathBuf,
    tmp: PathBuf,
}

impl SpawnDirs {
    fn create(base: &std::path::Path) -> std::io::Result<Self> {
        let mut builder = tempfile::Builder::new();
        builder.prefix(SPAWN_DIR_PREFIX);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(std::fs::Permissions::from_mode(0o700));
        }
        // Until `keep`, a failure below removes the half-built tree.
        let root = builder.tempdir_in(base)?;
        let [cwd, home, tmp] = ["cwd", "home", "tmp"].map(|name| root.path().join(name));
        for dir in [&cwd, &home, &tmp] {
            private_dir_builder().create(dir)?;
        }
        Ok(Self {
            root: root.keep(),
            cwd,
            home,
            tmp,
        })
    }

    fn cwd(&self) -> &std::path::Path {
        &self.cwd
    }

    fn home(&self) -> &std::path::Path {
        &self.home
    }

    fn tmp(&self) -> &std::path::Path {
        &self.tmp
    }
}

impl Drop for SpawnDirs {
    fn drop(&mut self) {
        // A program may leave directories it made read-only, which
        // `remove_dir_all` cannot empty.
        make_tree_removable(&self.root);
        if let Err(e) = std::fs::remove_dir_all(&self.root) {
            eprintln!(
                "command: could not remove spawn directory \"{}\": {e}",
                self.root.display()
            );
        }
    }
}

fn private_dir_builder() -> std::fs::DirBuilder {
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
}

/// Give the owner full access to every directory below `root`,
/// without following symlinks, so the whole tree can be removed.
fn make_tree_removable(root: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        // Iterative, since a program can nest directories deeper than
        // the stack would allow for recursion.
        let mut pending = vec![root.to_path_buf()];
        while let Some(dir) = pending.pop() {
            let Ok(meta) = std::fs::symlink_metadata(&dir) else {
                continue;
            };
            if !meta.is_dir() {
                continue;
            }
            let mode = meta.permissions().mode();
            if mode & 0o700 != 0o700 {
                let _ =
                    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(mode | 0o700));
            }
            if let Ok(entries) = std::fs::read_dir(&dir) {
                pending.extend(entries.flatten().map(|entry| entry.path()));
            }
        }
    }
    #[cfg(not(unix))]
    let _ = root;
}

// =========================================================
// Environment
//
// The child inherits nothing but locale and time zone settings. Its
// `PATH` is fixed, so a bare `binary` name and the program's own
// subprocess lookups resolve the same way on every machine, and its
// `HOME` and `TMPDIR` are the spawn's own empty directories, so
// programs that find configuration through them see none of the
// user's files.
// =========================================================

/// The guest's env overrides, checked against the keys and value
/// constraints the rule declares. A `path-under` value is replaced by
/// its canonical path, so the program reads the path that was checked.
/// A value checked only by a pattern (`regex`, `glob`, `any-string`)
/// may not contain a path separator: it can then name nothing but
/// something relative to the program's working directory or `HOME`.
fn check_env_overrides(
    rule: &CompiledCommandRule,
    overrides: &[(String, String)],
    writable: &WritableAreas,
) -> Result<Vec<(std::ffi::OsString, std::ffi::OsString)>, CommandCapError> {
    use argv_matcher::CompiledArgvConstraint as C;

    let deny = |message: String| Err(CommandCapError::PermissionDenied(message));
    let mut checked = Vec::with_capacity(overrides.len());
    for (key, value) in overrides {
        let Some((_, constraint)) = rule.env.iter().find(|(declared, _)| declared == key) else {
            return deny(format!(
                "the rule for \"{}\" does not declare the environment variable {key}",
                rule.binary
            ));
        };
        if value.contains('\0') {
            return deny(format!("the value of {key} contains a NUL byte"));
        }
        let value = match constraint {
            C::PathUnder(root) => checked_path_value(key, value, root, writable)?,
            other => {
                if !argv_matcher::value_matches(other, value) {
                    return deny(format!(
                        "the value of {key} does not meet the rule's constraint"
                    ));
                }
                if matches!(other, C::Regex(_) | C::Glob(_) | C::AnyString)
                    && value.chars().any(std::path::is_separator)
                {
                    return deny(format!(
                        "the value of {key} contains a path separator; declare it as \
                         path-under to pass a path"
                    ));
                }
                value.into()
            }
        };
        checked.push((key.into(), value));
    }
    Ok(checked)
}

/// A `path-under` env value: absolute, without `..`, under the
/// declared root, and neither in nor around an area the gadget can
/// write. Returns the canonical path.
fn checked_path_value(
    key: &str,
    value: &str,
    root: &std::path::Path,
    writable: &WritableAreas,
) -> Result<std::ffi::OsString, CommandCapError> {
    let requested = std::path::Path::new(value);
    if requested
        .components()
        .any(|component| component == std::path::Component::ParentDir)
    {
        return Err(CommandCapError::PermissionDenied(format!(
            "the value of {key} contains \"..\""
        )));
    }
    let canonical =
        crate::wasm::path_safety::canonical_under_root(requested, root).map_err(|e| {
            CommandCapError::PermissionDenied(format!(
                "the value of {key} is not inside the declared root: {e}"
            ))
        })?;
    writable
        .deny_touching(&canonical, Scope::Configuration)
        .map_err(|reason| {
            CommandCapError::PermissionDenied(format!("the value of {key} {reason}"))
        })?;
    Ok(canonical.into_os_string())
}

/// The child's `PATH`. macOS-specific: the system directories plus
/// the Homebrew prefixes of Intel (`/usr/local/bin`) and Apple
/// silicon (`/opt/homebrew/bin`) Macs, where the command-line tools
/// gadgets run are usually installed. Other platforms need their own
/// list (todo
/// `01M5DN71VBN0H0AVSQT2GMDK49-decide-how-command-confinement-works-on-linux-and-windows.md`).
const COMMAND_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin:/usr/local/bin:/opt/homebrew/bin";

/// Whether the host's value of `key` passes to the child.
/// `__CF_USER_TEXT_ENCODING` is macOS-specific: CoreFoundation reads
/// the default text encoding from it. Other platforms need their own
/// choice (todo
/// `01M5DN71VBN0H0AVSQT2GMDK49-decide-how-command-confinement-works-on-linux-and-windows.md`).
fn is_inherited_env_key(key: &std::ffi::OsStr) -> bool {
    let Some(key) = key.to_str() else {
        return false;
    };
    matches!(key, "LANG" | "LANGUAGE" | "TZ" | "__CF_USER_TEXT_ENCODING") || key.starts_with("LC_")
}

/// The child's environment: the inherited locale keys, the fixed
/// `PATH`, the spawn's `HOME` and `TMPDIR`, then `overrides`, each
/// replacing a key of the same name.
fn build_command_env(
    inherited: impl IntoIterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
    spawn_dirs: &SpawnDirs,
    overrides: &[(std::ffi::OsString, std::ffi::OsString)],
) -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
    use std::ffi::OsString;

    let mut env: Vec<(OsString, OsString)> = inherited
        .into_iter()
        .filter(|(key, _)| is_inherited_env_key(key))
        .collect();
    env.push(("PATH".into(), COMMAND_PATH.into()));
    env.push(("HOME".into(), spawn_dirs.home().into()));
    env.push(("TMPDIR".into(), spawn_dirs.tmp().into()));

    for (key, value) in overrides {
        let key = key.clone();
        env.retain(|(existing, _)| existing != &key);
        env.push((key, value.clone()));
    }

    env
}

// =========================================================
// Process spawn with timeout and output capping
// =========================================================

async fn run_child_with_caps(
    binary: &str,
    args: &[String],
    cwd: &std::path::Path,
    env: Vec<(std::ffi::OsString, std::ffi::OsString)>,
    stdin_bytes: Option<Vec<u8>>,
    timeout_ms: u64,
    max_output_bytes: u64,
) -> Result<CommandResult, CommandCapError> {
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt;

    let mut command = tokio::process::Command::new(binary);
    command
        .args(args)
        .current_dir(cwd)
        .env_clear()
        .envs(env)
        .stdin(if stdin_bytes.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(unix)]
    command.process_group(0);

    let mut child = command
        .spawn()
        .map_err(|e| CommandCapError::SpawnFailed(format!("spawn `{binary}`: {e}")))?;

    if let Some(bytes) = stdin_bytes
        && let Some(mut child_stdin) = child.stdin.take()
        && let Err(e) = child_stdin.write_all(&bytes).await
    {
        let _ = kill_child_with_grace(&mut child).await;
        return Err(CommandCapError::SpawnFailed(format!("writing stdin: {e}")));
    }

    let stdout = child
        .stdout
        .take()
        .expect("stdout configured to piped on spawn");
    let stderr = child
        .stderr
        .take()
        .expect("stderr configured to piped on spawn");

    let stdout_task = tokio::spawn(read_capped(stdout, max_output_bytes));
    let stderr_task = tokio::spawn(read_capped(stderr, max_output_bytes));

    let timeout = tokio::time::Duration::from_millis(timeout_ms);
    let wait_result = tokio::time::timeout(timeout, child.wait()).await;

    match wait_result {
        Ok(Ok(status)) => {
            let stdout_join = stdout_task.await.unwrap_or_else(|e| {
                Ok((Vec::new(), false, format!("stdout reader panicked: {e}")))
            });
            let stderr_join = stderr_task.await.unwrap_or_else(|e| {
                Ok((Vec::new(), false, format!("stderr reader panicked: {e}")))
            });

            let (stdout_bytes, stdout_over, _) = stdout_join
                .unwrap_or_else(|e| (Vec::new(), false, format!("stdout read error: {e}")));
            let (stderr_bytes, stderr_over, _) = stderr_join
                .unwrap_or_else(|e| (Vec::new(), false, format!("stderr read error: {e}")));

            if stdout_over || stderr_over {
                return Err(CommandCapError::OutputTooLarge(stdout_bytes, stderr_bytes));
            }

            #[cfg(unix)]
            let signal = {
                use std::os::unix::process::ExitStatusExt;
                status.signal().map(signal_name)
            };
            #[cfg(not(unix))]
            let signal = None;

            Ok(CommandResult {
                exit_code: status.code(),
                signal,
                timed_out: false,
                stdout: stdout_bytes,
                stderr: stderr_bytes,
            })
        }
        Ok(Err(e)) => Err(CommandCapError::SpawnFailed(format!(
            "waiting for child: {e}"
        ))),
        Err(_elapsed) => {
            let _ = kill_child_with_grace(&mut child).await;

            let stdout_bytes = stdout_task
                .await
                .ok()
                .and_then(|r| r.ok().map(|(b, _, _)| b))
                .unwrap_or_default();
            let stderr_bytes = stderr_task
                .await
                .ok()
                .and_then(|r| r.ok().map(|(b, _, _)| b))
                .unwrap_or_default();

            let _ = (stdout_bytes, stderr_bytes);
            Err(CommandCapError::Timeout)
        }
    }
}

async fn read_capped<R>(reader: R, cap: u64) -> std::io::Result<(Vec<u8>, bool, String)>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    use tokio::io::AsyncReadExt;

    let mut buf = Vec::new();
    let cap_usize = cap.try_into().unwrap_or(usize::MAX);

    let probe_limit = cap.saturating_add(1);
    let mut limited = reader.take(probe_limit);
    limited.read_to_end(&mut buf).await?;

    let overflowed = buf.len() as u64 > cap;
    if overflowed {
        buf.truncate(cap_usize);
    }

    Ok((buf, overflowed, String::new()))
}

async fn kill_child_with_grace(child: &mut tokio::process::Child) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        if let Some(pid) = child.id() {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGTERM);
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.start_kill();
    }

    let grace = tokio::time::Duration::from_millis(250);
    if tokio::time::timeout(grace, child.wait()).await.is_ok() {
        return Ok(());
    }

    #[cfg(unix)]
    {
        if let Some(pid) = child.id() {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.start_kill();
    }

    let _ = child.wait().await;
    Ok(())
}

#[cfg(unix)]
fn signal_name(signal: i32) -> String {
    match signal {
        libc::SIGTERM => "SIGTERM".to_string(),
        libc::SIGKILL => "SIGKILL".to_string(),
        libc::SIGINT => "SIGINT".to_string(),
        libc::SIGHUP => "SIGHUP".to_string(),
        libc::SIGSEGV => "SIGSEGV".to_string(),
        libc::SIGABRT => "SIGABRT".to_string(),
        libc::SIGPIPE => "SIGPIPE".to_string(),
        n => format!("SIG({n})"),
    }
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ─── CommandPermissions ───────────────────────────────

    #[test]
    fn command_permissions_stores_rules() {
        let perms = CommandPermissions {
            rules: vec![CommandPermissionDef {
                binary: "/usr/bin/echo".into(),
                argv: vec![],
                cwd: None,
                timeout_ms_max: None,
                max_output_bytes: None,
                max_stdin_bytes: None,
                env: Default::default(),
            }],
        };
        assert_eq!(perms.rules.len(), 1);
        assert_eq!(perms.rules[0].binary, "/usr/bin/echo");
    }

    #[test]
    fn command_permissions_empty_rules() {
        let perms = CommandPermissions { rules: vec![] };
        assert!(perms.rules.is_empty());
    }

    // ─── timeout / output clamp arithmetic ────────────────

    #[test]
    fn timeout_clamps_to_max() {
        assert_eq!(
            resolve_timeout_ms(Some(999_999_999)),
            MAX_COMMAND_TIMEOUT_MS
        );
    }

    #[test]
    fn timeout_default_when_none() {
        assert_eq!(resolve_timeout_ms(None), DEFAULT_COMMAND_TIMEOUT_MS);
    }

    #[test]
    fn timeout_passes_through_in_range_value() {
        assert_eq!(resolve_timeout_ms(Some(5_000)), 5_000);
    }

    #[test]
    fn output_bytes_clamps_to_max() {
        assert_eq!(
            resolve_max_output_bytes(Some(u64::MAX)),
            MAX_COMMAND_OUTPUT_BYTES
        );
    }

    #[test]
    fn output_bytes_default_when_none() {
        assert_eq!(resolve_max_output_bytes(None), DEFAULT_COMMAND_OUTPUT_BYTES);
    }

    #[test]
    fn output_bytes_passes_through_in_range_value() {
        assert_eq!(resolve_max_output_bytes(Some(1024)), 1024);
    }

    // ─── permission denied ────────────────────────────────

    #[test]
    fn run_with_empty_rules_returns_permission_denied() {
        let cap = CommandCap::from_compiled_rules(vec![], PathBuf::from("/tmp/test"));
        let result = cap.run(
            "/bin/echo",
            CommandOptions {
                args: vec!["hello".into()],
                cwd: None,
                env: vec![],
                stdin: None,
                timeout_ms: None,
                max_output_bytes: None,
            },
        );
        match result {
            Err(CommandCapError::PermissionDenied(msg)) => {
                assert!(msg.contains("/bin/echo"));
            }
            other => panic!("expected PermissionDenied, got {other:?}"),
        }
    }

    // ─── default working directory ────────────────────────

    /// A cap over `/bin/pwd`, `/bin/ls -A` and `/bin/sh -c <script>`,
    /// with the gadget's data directory in the host layout below
    /// `root`.
    fn spawning_cap(root: &std::path::Path) -> CommandCap {
        use argv_matcher::CompiledArgvConstraint as C;
        let paths = gadget_paths(root, "spawner");
        CommandCap::from_compiled_rules(
            vec![
                CompiledCommandRule {
                    binary: "/bin/pwd".into(),
                    argv: vec![],
                    cwd: None,
                    env: vec![],
                },
                CompiledCommandRule {
                    binary: "/bin/ls".into(),
                    argv: vec![C::Literal("-A".into())],
                    cwd: None,
                    env: vec![],
                },
                CompiledCommandRule {
                    binary: "/bin/sh".into(),
                    argv: vec![C::Literal("-c".into()), C::AnyString],
                    cwd: None,
                    env: vec![],
                },
                CompiledCommandRule {
                    binary: "/usr/bin/env".into(),
                    argv: vec![],
                    cwd: None,
                    env: vec![],
                },
            ],
            paths.gadget_data,
        )
    }

    fn stdout_line(result: &CommandResult) -> String {
        String::from_utf8_lossy(&result.stdout)
            .trim_end()
            .to_string()
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn default_cwd_is_outside_the_gadget_data_dir() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let cap = spawning_cap(root.path());

        let cwd = stdout_line(&cap.run("/bin/pwd", options(&[])).expect("pwd runs"));

        let data = gadget_paths(root.path(), "spawner").gadget_data;
        std::fs::create_dir_all(&data).expect("data dir");
        let data = data.canonicalize().expect("canonical data dir");
        assert!(
            !std::path::Path::new(&cwd).starts_with(&data),
            "the default cwd `{cwd}` lies in the gadget's data directory"
        );
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn each_spawn_gets_its_own_cwd() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let cap = spawning_cap(root.path());

        let first = stdout_line(&cap.run("/bin/pwd", options(&[])).expect("pwd runs"));
        let second = stdout_line(&cap.run("/bin/pwd", options(&[])).expect("pwd runs"));

        assert_ne!(first, second, "two spawns shared the cwd `{first}`");
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn default_cwd_is_removed_after_the_run() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let cap = spawning_cap(root.path());

        let cwd = stdout_line(&cap.run("/bin/pwd", options(&[])).expect("pwd runs"));

        assert!(
            !std::path::Path::new(&cwd).exists(),
            "the default cwd `{cwd}` outlived the run"
        );
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn no_exec_cwd_is_created_in_the_gadget_data_dir() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let cap = spawning_cap(root.path());

        cap.run("/bin/pwd", options(&[])).expect("pwd runs");

        let data = gadget_paths(root.path(), "spawner").gadget_data;
        assert!(
            !data.join("exec-cwd").exists(),
            "a run created `exec-cwd/` in the gadget's data directory"
        );
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn default_cwd_is_empty() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let cap = spawning_cap(root.path());
        cap.run("/bin/sh", options(&["-c", "touch left-behind"]))
            .expect("sh runs");

        let listing = cap.run("/bin/ls", options(&["-A"])).expect("ls runs");

        assert_eq!(stdout_line(&listing), "", "the default cwd is not empty");
    }

    /// A cap whose spawn directories go below a base the test owns,
    /// so the test can see whether a run leaves anything behind.
    fn cap_with_spawn_base(root: &std::path::Path) -> (CommandCap, PathBuf) {
        let base = root.join("spawn-base");
        std::fs::create_dir_all(&base).expect("spawn base");
        (spawning_cap(root).with_spawn_base(base.clone()), base)
    }

    fn entries(dir: &std::path::Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .expect("read dir")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn spawn_dirs_are_removed_after_a_timeout() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let (cap, base) = cap_with_spawn_base(root.path());
        let mut opts = options(&["-c", "sleep 5"]);
        opts.timeout_ms = Some(150);

        let result = cap.run("/bin/sh", opts);

        assert!(
            matches!(result, Err(CommandCapError::Timeout)),
            "{result:?}"
        );
        assert_eq!(entries(&base), Vec::<String>::new());
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn spawn_dirs_are_removed_after_too_much_output() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let (cap, base) = cap_with_spawn_base(root.path());
        let mut opts = options(&["-c", "head -c 1024 /dev/zero"]);
        opts.max_output_bytes = Some(64);

        let result = cap.run("/bin/sh", opts);

        assert!(
            matches!(result, Err(CommandCapError::OutputTooLarge(..))),
            "{result:?}"
        );
        assert_eq!(entries(&base), Vec::<String>::new());
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn spawn_dirs_are_removed_after_a_failed_spawn() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let base = root.path().join("spawn-base");
        std::fs::create_dir_all(&base).expect("spawn base");
        let missing = root.path().join("no-such-program");
        let cap = CommandCap::from_compiled_rules(
            vec![CompiledCommandRule {
                binary: missing.to_string_lossy().into_owned(),
                argv: vec![],
                cwd: None,
                env: vec![],
            }],
            gadget_paths(root.path(), "spawner").gadget_data,
        )
        .with_spawn_base(base.clone());

        let result = cap.run(&missing.to_string_lossy(), options(&[]));

        assert!(
            matches!(result, Err(CommandCapError::SpawnFailed(_))),
            "{result:?}"
        );
        assert_eq!(entries(&base), Vec::<String>::new());
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn spawn_dirs_with_read_only_subdirectories_are_removed() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let (cap, base) = cap_with_spawn_base(root.path());

        let result = cap
            .run(
                "/bin/sh",
                options(&[
                    "-c",
                    "mkdir -p a/b && touch a/b/f && chmod 555 a/b a && chmod 000 a",
                ]),
            )
            .expect("sh runs");

        assert_eq!(
            result.exit_code,
            Some(0),
            "{:?}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(entries(&base), Vec::<String>::new());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn denied_call_creates_no_spawn_dir() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let (cap, base) = cap_with_spawn_base(root.path());
        let mut guest_cwd = options(&[]);
        guest_cwd.cwd = Some(root.path().to_string_lossy().into_owned());
        let mut undeclared_env = options(&[]);
        undeclared_env.env = vec![("GIT_DIR".into(), "x".into())];
        let denials = [
            ("argv no rule accepts", options(&["unexpected-argument"])),
            ("cwd without a rule cwd", guest_cwd),
            ("env not declared", undeclared_env),
        ];

        for (case, opts) in denials {
            let result = cap.run("/bin/pwd", opts);

            assert!(
                matches!(result, Err(CommandCapError::PermissionDenied(_))),
                "{case}: {result:?}"
            );
            assert_eq!(entries(&base), Vec::<String>::new(), "{case}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn spawn_dirs_are_private_to_the_user() {
        use std::os::unix::fs::PermissionsExt;
        let base = tempfile::TempDir::new().expect("tempdir");

        let dirs = SpawnDirs::create(base.path()).expect("spawn dirs");

        let root = dirs.cwd().parent().expect("cwd has a parent");
        for dir in [root, dirs.cwd()] {
            let mode = std::fs::metadata(dir)
                .expect("metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o700, "{}", dir.display());
        }
        assert!(
            root.file_name()
                .expect("root has a name")
                .to_string_lossy()
                .starts_with(SPAWN_DIR_PREFIX)
        );
    }

    #[test]
    fn dropping_spawn_dirs_removes_them() {
        let base = tempfile::TempDir::new().expect("tempdir");
        let dirs = SpawnDirs::create(base.path()).expect("spawn dirs");
        std::fs::write(dirs.cwd().join("file"), b"x").expect("write");

        drop(dirs);

        assert_eq!(entries(base.path()), Vec::<String>::new());
    }

    #[test]
    fn dropping_spawn_dirs_that_are_already_gone_does_not_panic() {
        let base = tempfile::TempDir::new().expect("tempdir");
        let dirs = SpawnDirs::create(base.path()).expect("spawn dirs");
        std::fs::remove_dir_all(dirs.cwd().parent().expect("root")).expect("remove");

        drop(dirs);

        assert_eq!(entries(base.path()), Vec::<String>::new());
    }

    #[test]
    fn making_a_missing_tree_removable_is_a_no_op() {
        let base = tempfile::TempDir::new().expect("tempdir");
        make_tree_removable(&base.path().join("missing"));
    }

    #[test]
    fn a_missing_spawn_base_fails_the_spawn() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let cap = spawning_cap(root.path()).with_spawn_base(root.path().join("missing"));

        let result = cap.run("/bin/pwd", options(&[]));

        assert!(
            matches!(result, Err(CommandCapError::SpawnFailed(_))),
            "{result:?}"
        );
    }

    #[test]
    fn spawn_dirs_in_a_missing_base_fail_to_create() {
        let base = tempfile::TempDir::new().expect("tempdir");
        assert!(SpawnDirs::create(&base.path().join("missing")).is_err());
    }

    // ─── the child's environment ──────────────────────────

    /// The child's environment as `/usr/bin/env` prints it, which adds
    /// nothing of its own (a shell would add `PWD`, `SHLVL` and `_`).
    fn child_env(cap: &CommandCap) -> Vec<(String, String)> {
        let result = cap.run("/usr/bin/env", options(&[])).expect("env runs");
        String::from_utf8_lossy(&result.stdout)
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    fn child_var(env: &[(String, String)], key: &str) -> Option<String> {
        env.iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.clone())
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn child_env_has_only_allowlisted_keys() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let cap = spawning_cap(root.path());

        let unexpected: Vec<String> = child_env(&cap)
            .into_iter()
            .map(|(key, _)| key)
            .filter(|key| {
                !matches!(
                    key.as_str(),
                    "LANG"
                        | "LANGUAGE"
                        | "TZ"
                        | "__CF_USER_TEXT_ENCODING"
                        | "PATH"
                        | "HOME"
                        | "TMPDIR"
                ) && !key.starts_with("LC_")
            })
            .collect();

        assert_eq!(
            unexpected,
            Vec::<String>::new(),
            "keys outside the allowlist"
        );
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn child_path_is_the_fixed_path() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let cap = spawning_cap(root.path());

        let path = child_var(&child_env(&cap), "PATH");

        assert_eq!(
            path.as_deref(),
            Some("/usr/bin:/bin:/usr/sbin:/sbin:/usr/local/bin:/opt/homebrew/bin")
        );
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn child_home_and_tmpdir_are_empty_spawn_dirs() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let (cap, base) = cap_with_spawn_base(root.path());
        let base = base.canonicalize().expect("canonical base");

        let result = cap
            .run(
                "/bin/sh",
                options(&[
                    "-c",
                    "cd \"$HOME\" && pwd -P && ls -A && cd \"$TMPDIR\" && pwd -P && ls -A",
                ]),
            )
            .expect("sh runs");

        let output = String::from_utf8_lossy(&result.stdout).into_owned();
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(
            lines.len(),
            2,
            "HOME or TMPDIR is missing or not empty: {output:?}"
        );
        for dir in lines {
            assert!(
                std::path::Path::new(dir).starts_with(&base),
                "`{dir}` is not a spawn directory"
            );
        }
    }

    #[test]
    fn credentials_are_not_inherited() {
        let dirs = SpawnDirs::create(&std::env::temp_dir()).expect("spawn dirs");
        let inherited = [
            "PGPASSWORD",
            "MYSQL_PWD",
            "DATABASE_URL",
            "SENTRY_DSN",
            "KUBECONFIG",
            "GITHUB_TOKEN",
            "SSH_AUTH_SOCK",
            "USER",
        ]
        .map(|key| (key.into(), "secret".into()));

        let env = build_command_env(inherited, &dirs, &[]);

        let leaked: Vec<_> = env
            .iter()
            .filter(|(_, value)| value == "secret")
            .map(|(key, _)| key.clone())
            .collect();
        assert_eq!(leaked, Vec::<std::ffi::OsString>::new());
    }

    #[test]
    fn locale_and_time_zone_are_inherited() {
        let dirs = SpawnDirs::create(&std::env::temp_dir()).expect("spawn dirs");
        let inherited = [
            ("LANG", "de_DE.UTF-8"),
            ("LANGUAGE", "de"),
            ("LC_CTYPE", "UTF-8"),
            ("LC_ALL", "C"),
            ("TZ", "Europe/Berlin"),
            ("__CF_USER_TEXT_ENCODING", "0x1F5:0x0:0x0"),
        ]
        .map(|(key, value)| (key.into(), value.into()));

        let env = build_command_env(inherited.clone(), &dirs, &[]);

        for (key, value) in inherited {
            assert!(
                env.contains(&(key.clone(), value)),
                "{key:?} was not inherited"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_key_that_is_not_utf8_is_not_inherited() {
        use std::os::unix::ffi::OsStringExt;
        let dirs = SpawnDirs::create(&std::env::temp_dir()).expect("spawn dirs");
        let key = std::ffi::OsString::from_vec(b"LC_\xff".to_vec());

        let env = build_command_env([(key.clone(), "x".into())], &dirs, &[]);

        assert!(!env.iter().any(|(name, _)| name == &key));
    }

    #[test]
    fn host_path_home_and_tmpdir_are_replaced() {
        let dirs = SpawnDirs::create(&std::env::temp_dir()).expect("spawn dirs");
        let inherited = [
            ("PATH", "/Users/me/evil:/usr/bin"),
            ("HOME", "/Users/me"),
            ("TMPDIR", "/var/folders/xy/T/"),
        ]
        .map(|(key, value)| (key.into(), value.into()));

        let env = build_command_env(inherited, &dirs, &[]);

        let value = |key: &str| {
            env.iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.clone())
        };
        assert_eq!(value("PATH"), Some(COMMAND_PATH.into()));
        assert_eq!(value("HOME"), Some(dirs.home().as_os_str().to_owned()));
        assert_eq!(value("TMPDIR"), Some(dirs.tmp().as_os_str().to_owned()));
    }

    #[test]
    fn overrides_replace_existing_env_vars() {
        let dirs = SpawnDirs::create(&std::env::temp_dir()).expect("spawn dirs");
        let inherited = [("LANG".into(), "de_DE.UTF-8".into())];

        let env = build_command_env(inherited, &dirs, &[("LANG".into(), "C".into())]);

        let langs: Vec<_> = env.iter().filter(|(key, _)| key == "LANG").collect();
        assert_eq!(langs, vec![&("LANG".into(), "C".into())]);
    }

    // ─── declared environment overrides ───────────────────

    /// A cap over `/usr/bin/env` whose rule declares the env keys of
    /// `declaration` (TOML table body), compiled for a gadget laid out
    /// below `root`.
    fn env_cap(root: &std::path::Path, declaration: &str) -> CommandCap {
        let manifest = crate::wasm::manifest::Manifest::parse(&format!(
            r#"
            [gadget]
            id = "env"
            name = "Env"
            description = "Env"
            version = "0.1.0"
            wasm = "env.wasm"
            icon = "heroicons:beaker"
            [[permissions.command]]
            binary = "/usr/bin/env"
            [permissions.command.env]
            {declaration}
            "#
        ))
        .expect("test manifest parses");
        let rules = manifest.permissions.expect("permissions").command;
        let paths = gadget_paths(root, "env");
        CommandCap::new(&rules, &paths, paths.gadget_data.clone()).expect("rules compile")
    }

    fn with_env(pairs: &[(&str, &str)]) -> CommandOptions {
        let mut opts = options(&[]);
        opts.env = pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        opts
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn undeclared_env_override_is_denied() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let cap = env_cap(root.path(), r#"GIT_AUTHOR_NAME = { kind = "any-string" }"#);

        assert_denied(cap.run("/usr/bin/env", with_env(&[("GIT_COMMITTER_NAME", "x")])));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn ld_preload_override_is_denied() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let cap = env_cap(root.path(), r#"GIT_AUTHOR_NAME = { kind = "any-string" }"#);

        assert_denied(cap.run("/usr/bin/env", with_env(&[("LD_PRELOAD", "evil.so")])));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn path_override_is_denied() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let cap = env_cap(root.path(), r#"GIT_AUTHOR_NAME = { kind = "any-string" }"#);

        assert_denied(cap.run("/usr/bin/env", with_env(&[("PATH", "/tmp")])));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn env_key_case_must_match_the_declaration() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let cap = env_cap(root.path(), r#"GIT_AUTHOR_NAME = { kind = "any-string" }"#);

        assert_denied(cap.run("/usr/bin/env", with_env(&[("git_author_name", "x")])));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn env_value_violating_its_constraint_is_denied() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let cap = env_cap(
            root.path(),
            r#"LC_ALL = { kind = "enum", values = ["C", "en_US.UTF-8"] }"#,
        );

        assert_denied(cap.run("/usr/bin/env", with_env(&[("LC_ALL", "de_DE.UTF-8")])));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn env_value_with_nul_is_denied() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let cap = env_cap(root.path(), r#"GIT_AUTHOR_NAME = { kind = "any-string" }"#);

        assert_denied(cap.run("/usr/bin/env", with_env(&[("GIT_AUTHOR_NAME", "a\0b")])));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn pattern_env_value_with_a_path_separator_is_denied() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let cap = env_cap(root.path(), r#"GIT_DIR = { kind = "any-string" }"#);

        assert_denied(cap.run("/usr/bin/env", with_env(&[("GIT_DIR", "../repo/.git")])));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn path_under_env_value_outside_root_is_denied() {
        let root = tempfile::TempDir::new().expect("tempdir");
        projects(root.path());
        std::fs::create_dir_all(root.path().join("elsewhere")).expect("elsewhere");
        let cap = env_cap(
            root.path(),
            r#"GIT_DIR = { kind = "path-under", root = "${home}/projects" }"#,
        );

        let outside = root.path().join("elsewhere");
        assert_denied(cap.run(
            "/usr/bin/env",
            with_env(&[("GIT_DIR", &outside.to_string_lossy())]),
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn path_under_env_value_with_dotdot_is_denied() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let repo = projects(root.path()).join("repo");
        std::fs::create_dir_all(&repo).expect("repo");
        let cap = env_cap(
            root.path(),
            r#"GIT_DIR = { kind = "path-under", root = "${home}/projects" }"#,
        );

        let value = repo.join("../repo");
        assert_denied(cap.run(
            "/usr/bin/env",
            with_env(&[("GIT_DIR", &value.to_string_lossy())]),
        ));
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn path_under_env_value_reaches_the_child_canonical() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let repo = projects(root.path()).join("repo");
        std::fs::create_dir_all(&repo).expect("repo");
        std::os::unix::fs::symlink(&repo, projects(root.path()).join("alias")).expect("link");
        let cap = env_cap(
            root.path(),
            r#"GIT_DIR = { kind = "path-under", root = "${home}/projects" }"#,
        );

        let value = projects(root.path()).join("alias");
        let result = cap
            .run(
                "/usr/bin/env",
                with_env(&[("GIT_DIR", &value.to_string_lossy())]),
            )
            .expect("env runs");

        let env = String::from_utf8_lossy(&result.stdout).into_owned();
        let canonical = repo.canonicalize().expect("canonical repo");
        assert!(
            env.lines()
                .any(|line| line == format!("GIT_DIR={}", canonical.display())),
            "{env}"
        );
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn declared_env_override_reaches_the_child() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let cap = env_cap(root.path(), r#"GIT_AUTHOR_NAME = { kind = "any-string" }"#);

        let result = cap
            .run("/usr/bin/env", with_env(&[("GIT_AUTHOR_NAME", "Jo Doe")]))
            .expect("env runs");

        assert!(String::from_utf8_lossy(&result.stdout).contains("GIT_AUTHOR_NAME=Jo Doe\n"));
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn declared_override_replaces_a_base_key() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let cap = env_cap(root.path(), r#"LANG = { kind = "literal", value = "C" }"#);

        let result = cap
            .run("/usr/bin/env", with_env(&[("LANG", "C")]))
            .expect("env runs");

        let stdout = String::from_utf8_lossy(&result.stdout).into_owned();
        let langs: Vec<&str> = stdout
            .lines()
            .filter(|line| line.starts_with("LANG="))
            .collect();
        assert_eq!(langs, vec!["LANG=C"]);
    }

    // ─── the gadget's writable areas ──────────────────────

    /// Compile one rule (TOML body below `binary = "/bin/pwd"`) for the
    /// gadget `w` laid out below `root` (app data at `root/app-data`).
    fn cap_from_rule(
        root: &std::path::Path,
        body: &str,
        paths: &crate::paths::GadgetPaths,
    ) -> anyhow::Result<CommandCap> {
        let manifest = crate::wasm::manifest::Manifest::parse(&format!(
            r#"
            [gadget]
            id = "w"
            name = "W"
            description = "W"
            version = "0.1.0"
            wasm = "w.wasm"
            icon = "heroicons:beaker"
            [[permissions.command]]
            binary = "/bin/pwd"
            {body}
            "#
        ))?;
        let rules = manifest.permissions.expect("permissions").command;
        let _ = root;
        CommandCap::new(&rules, paths, paths.gadget_data.clone())
    }

    /// `root/app-data` with the gadget's data, another gadget's data and
    /// the installed packages, created.
    fn app_data(root: &std::path::Path) -> PathBuf {
        let app_data = root.join("app-data");
        for dir in ["gadget-home/w/sql", "gadget-home/other", "gadgets"] {
            std::fs::create_dir_all(app_data.join(dir)).expect("app data dirs");
        }
        app_data
    }

    fn load_error(root: &std::path::Path, body: &str) -> String {
        match cap_from_rule(root, body, &gadget_paths(root, "w")) {
            Ok(_) => "loaded".to_string(),
            Err(e) => format!("{e:#}"),
        }
    }

    #[test]
    fn cwd_root_resolving_into_gadget_data_is_rejected() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let app_data = app_data(root.path());

        let error = load_error(
            root.path(),
            &format!("cwd = \"{}\"", app_data.join("gadget-home/w/sql").display()),
        );

        assert!(error.contains("the gadget can write"), "{error}");
    }

    #[test]
    fn roots_in_other_gadgets_data_or_packages_are_rejected() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let app_data = app_data(root.path());
        let bodies = [
            format!("cwd = \"{}\"", app_data.join("gadget-home/other").display()),
            format!("cwd = \"{}\"", app_data.join("gadgets").display()),
            format!(
                "argv = [{{ kind = \"path-under\", root = \"{}\" }}]",
                app_data.join("gadget-home/other").display()
            ),
            format!(
                "[permissions.command.env]\nX = {{ kind = \"path-under\", root = \"{}\" }}",
                app_data.display()
            ),
        ];

        let loaded: Vec<&String> = bodies
            .iter()
            .filter(|body| load_error(root.path(), body) == "loaded")
            .collect();

        assert_eq!(loaded, Vec::<&String>::new());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_root_into_app_data_is_rejected() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let app_data = app_data(root.path());
        std::fs::create_dir_all(root.path().join("home")).expect("home");
        std::os::unix::fs::symlink(&app_data, root.path().join("home/link")).expect("link");

        let error = load_error(root.path(), "cwd = \"${home}/link/gadget-home\"");

        assert!(error.contains("the gadget can write"), "{error}");
    }

    /// APFS is case-insensitive by default: a root spelled in other case
    /// names the same directory.
    #[cfg(target_os = "macos")]
    #[test]
    fn case_variant_root_into_app_data_is_rejected() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let app_data = app_data(root.path());
        let variant = root.path().join("APP-DATA/GADGET-HOME/w/new");
        if !root.path().join("APP-DATA").exists() {
            return;
        }
        let _ = app_data;

        let error = load_error(root.path(), &format!("cwd = \"{}\"", variant.display()));

        assert!(error.contains("the gadget can write"), "{error}");
    }

    #[test]
    fn literal_values_resolving_into_app_data_are_rejected() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let app_data = app_data(root.path()).display().to_string();
        let bodies = [
            format!("argv = [{{ kind = \"literal\", value = \"{app_data}/gadget-home/w/x\" }}]"),
            format!(
                "argv = [{{ kind = \"literal\", value = \"--file={app_data}/./gadget-home/x\" }}]"
            ),
            format!(
                "[permissions.command.env]\nX = {{ kind = \"literal\", value = \"/etc:{app_data}//gadget-home/w\" }}"
            ),
            format!(
                "[permissions.command.env]\nX = {{ kind = \"enum\", values = [\"file://{app_data}/gadgets/w.torchsnap\"] }}"
            ),
        ];

        let loaded: Vec<&String> = bodies
            .iter()
            .filter(|body| load_error(root.path(), body) == "loaded")
            .collect();

        assert_eq!(loaded, Vec::<&String>::new());
    }

    #[cfg(unix)]
    #[test]
    fn literal_through_symlink_into_app_data_is_rejected() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let app_data = app_data(root.path());
        std::os::unix::fs::symlink(&app_data, root.path().join("alias")).expect("link");

        let error = load_error(
            root.path(),
            &format!(
                "argv = [{{ kind = \"literal\", value = \"{}/gadget-home/w\" }}]",
                root.path().join("alias").display()
            ),
        );

        assert!(error.contains("the gadget can write"), "{error}");
    }

    #[test]
    fn a_dev_gadget_package_is_off_limits_for_cwd_and_env() {
        let root = tempfile::TempDir::new().expect("tempdir");
        app_data(root.path());
        let package = root.path().join("repo/gadgets/w");
        std::fs::create_dir_all(package.join("scripts")).expect("package");
        let mut paths = gadget_paths(root.path(), "w");
        paths.gadget_archive = package.clone();
        let bodies = [
            format!("cwd = \"{}\"", package.join("scripts").display()),
            format!(
                "[permissions.command.env]\nX = {{ kind = \"literal\", value = \"{}/x\" }}",
                package.display()
            ),
        ];

        let loaded: Vec<&String> = bodies
            .iter()
            .filter(|body| cap_from_rule(root.path(), body, &paths).is_ok())
            .collect();

        assert_eq!(loaded, Vec::<&String>::new());
    }

    #[test]
    fn argv_may_name_files_in_the_gadget_package() {
        let root = tempfile::TempDir::new().expect("tempdir");
        app_data(root.path());
        let paths = gadget_paths(root.path(), "w");

        let cap = cap_from_rule(
            root.path(),
            "argv = [{ kind = \"literal\", value = \"${gadget-archive}\" }]",
            &paths,
        );

        assert!(cap.is_ok(), "{:?}", cap.err());
    }

    #[test]
    fn gadget_data_dir_outside_gadget_home_fails_to_load() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let mut paths = gadget_paths(root.path(), "w");
        paths.gadget_data = root.path().join("somewhere/w");

        let cap = cap_from_rule(root.path(), "", &paths);

        assert!(
            cap.is_err(),
            "loaded with a data directory outside gadget-home"
        );
    }

    /// A root that cannot be resolved at load (a dangling link) loads,
    /// and the check at run time denies it.
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn root_through_dangling_symlink_loads_but_is_denied_at_run() {
        let root = tempfile::TempDir::new().expect("tempdir");
        app_data(root.path());
        std::fs::create_dir_all(root.path().join("home")).expect("home");
        std::os::unix::fs::symlink(root.path().join("missing"), root.path().join("home/link"))
            .expect("link");
        let cap = cap_from_rule(
            root.path(),
            "cwd = \"${home}/link/repo\"",
            &gadget_paths(root.path(), "w"),
        )
        .expect("loads");

        assert_denied(cap.run("/bin/pwd", options(&[])));
    }

    /// A rule whose root contains the app data (here: the test root).
    fn broad_cap(root: &std::path::Path) -> CommandCap {
        cap_from_rule(
            root,
            &format!(
                "cwd = \"{}\"\nargv = [{{ kind = \"rest\", constraint = {{ kind = \"path-under\", root = \"{}\" }} }}]\n[permissions.command.env]\nGIT_DIR = {{ kind = \"path-under\", root = \"{}\" }}",
                root.display(),
                root.display(),
                root.display()
            ),
            &gadget_paths(root, "w"),
        )
        .expect("a root containing the app data loads")
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn guest_cwd_in_gadget_data_under_a_broad_root_is_denied() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let app_data = app_data(root.path());
        let cap = broad_cap(root.path());

        assert_denied(cap.run("/bin/pwd", with_cwd(&app_data.join("gadget-home/w/sql"))));
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn guest_cwd_through_symlink_into_app_data_is_denied() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let app_data = app_data(root.path());
        std::fs::create_dir_all(root.path().join("projects")).expect("projects");
        std::os::unix::fs::symlink(
            app_data.join("gadget-home/other"),
            root.path().join("projects/link"),
        )
        .expect("link");
        let cap = broad_cap(root.path());

        assert_denied(cap.run("/bin/pwd", with_cwd(&root.path().join("projects/link"))));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cwd_containing_the_app_data_dir_is_denied() {
        let root = tempfile::TempDir::new().expect("tempdir");
        app_data(root.path());
        let cap = broad_cap(root.path());

        assert_denied(cap.run("/bin/pwd", with_cwd(root.path())));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn default_cwd_at_a_root_containing_the_app_data_dir_is_denied() {
        let root = tempfile::TempDir::new().expect("tempdir");
        app_data(root.path());
        let cap = broad_cap(root.path());

        assert_denied(cap.run("/bin/pwd", options(&[])));
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn broad_root_containing_the_app_data_dir_allows_other_dirs() {
        let root = tempfile::TempDir::new().expect("tempdir");
        app_data(root.path());
        let projects = root.path().join("projects");
        std::fs::create_dir_all(&projects).expect("projects");
        let cap = broad_cap(root.path());

        let result = cap.run("/bin/pwd", with_cwd(&projects));

        assert!(result.is_ok(), "{result:?}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn path_under_argv_value_in_app_data_is_denied() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let app_data = app_data(root.path());
        std::fs::create_dir_all(root.path().join("projects")).expect("projects");
        let cap = broad_cap(root.path());
        let mut opts = with_cwd(&root.path().join("projects"));
        opts.args = vec![app_data.join("gadget-home/w/sql").display().to_string()];

        assert_denied(cap.run("/bin/pwd", opts));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn path_under_argv_value_containing_the_app_data_dir_is_denied() {
        let root = tempfile::TempDir::new().expect("tempdir");
        app_data(root.path());
        std::fs::create_dir_all(root.path().join("projects")).expect("projects");
        let cap = broad_cap(root.path());
        let mut opts = with_cwd(&root.path().join("projects"));
        opts.args = vec![root.path().display().to_string()];

        assert_denied(cap.run("/bin/pwd", opts));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn path_under_argv_value_with_dotdot_is_denied() {
        let root = tempfile::TempDir::new().expect("tempdir");
        app_data(root.path());
        std::fs::create_dir_all(root.path().join("projects/a")).expect("projects");
        let cap = broad_cap(root.path());
        let mut opts = with_cwd(&root.path().join("projects"));
        opts.args = vec![root.path().join("projects/a/../a").display().to_string()];

        assert_denied(cap.run("/bin/pwd", opts));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn path_under_env_value_in_app_data_is_denied() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let app_data = app_data(root.path());
        std::fs::create_dir_all(root.path().join("projects")).expect("projects");
        let cap = broad_cap(root.path());
        let mut opts = with_cwd(&root.path().join("projects"));
        opts.env = vec![(
            "GIT_DIR".into(),
            app_data.join("gadget-home/other").display().to_string(),
        )];

        assert_denied(cap.run("/bin/pwd", opts));
    }

    #[test]
    fn stale_spawn_dirs_are_swept() {
        let base = tempfile::TempDir::new().expect("tempdir");
        let stale = base.path().join(format!("{SPAWN_DIR_PREFIX}stale"));
        let fresh = base.path().join(format!("{SPAWN_DIR_PREFIX}fresh"));
        let unrelated = base.path().join("unrelated");
        for dir in [&stale, &fresh, &unrelated] {
            std::fs::create_dir_all(dir.join("home")).expect("dir");
        }
        let two_hours_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(7200);
        for dir in [&stale, &unrelated] {
            std::fs::File::open(dir)
                .expect("open dir")
                .set_modified(two_hours_ago)
                .expect("set mtime");
        }

        sweep_stale_spawn_dirs(base.path(), std::time::Duration::from_secs(3600));

        let mut left = entries(base.path());
        left.sort();
        assert_eq!(
            left,
            vec![format!("{SPAWN_DIR_PREFIX}fresh"), "unrelated".to_string()]
        );
    }

    #[test]
    fn sweeping_a_missing_base_does_nothing() {
        let base = tempfile::TempDir::new().expect("tempdir");
        sweep_stale_spawn_dirs(&base.path().join("missing"), std::time::Duration::ZERO);
    }

    #[test]
    fn without_a_package_only_the_app_data_is_writable() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let app_data = app_data(root.path());
        let writable = WritableAreas {
            app_data: app_data.clone(),
            package: None,
        };

        for scope in [Scope::Configuration, Scope::Argument] {
            assert_eq!(
                writable.holds(&app_data.join("x"), scope),
                Containment::Inside
            );
            assert_eq!(writable.holds(root.path(), scope), Containment::Outside);
        }
    }

    #[cfg(unix)]
    #[test]
    fn sweeping_leaves_symlinks_alone() {
        let base = tempfile::TempDir::new().expect("tempdir");
        let target = tempfile::TempDir::new().expect("target");
        let link = base.path().join(format!("{SPAWN_DIR_PREFIX}link"));
        std::os::unix::fs::symlink(target.path(), &link).expect("link");

        sweep_stale_spawn_dirs(base.path(), std::time::Duration::ZERO);

        assert!(link.symlink_metadata().is_ok());
        assert!(target.path().exists());
    }

    // ─── the rule's cwd root ──────────────────────────────

    /// A cap over `/bin/pwd` whose one rule declares `rule_cwd`, for a
    /// gadget laid out below `root` (home at `root/home`).
    fn pwd_cap(root: &std::path::Path, rule_cwd: Option<&str>) -> CommandCap {
        let paths = gadget_paths(root, "pwd");
        let rule = CommandPermissionDef {
            binary: "/bin/pwd".into(),
            argv: vec![],
            cwd: rule_cwd.map(str::to_string),
            timeout_ms_max: None,
            max_output_bytes: None,
            max_stdin_bytes: None,
            env: Default::default(),
        };
        CommandCap::new(&[rule], &paths, paths.gadget_data.clone()).expect("rule compiles")
    }

    fn with_cwd(cwd: &std::path::Path) -> CommandOptions {
        let mut opts = options(&[]);
        opts.cwd = Some(cwd.to_string_lossy().into_owned());
        opts
    }

    /// `root/home/projects`, created, as the rule root
    /// `${home}/projects` resolves it.
    fn projects(root: &std::path::Path) -> PathBuf {
        let projects = root.join("home/projects");
        std::fs::create_dir_all(&projects).expect("projects dir");
        projects
    }

    fn assert_denied(result: Result<CommandResult, CommandCapError>) {
        assert!(
            matches!(result, Err(CommandCapError::PermissionDenied(_))),
            "expected PermissionDenied, got {result:?}"
        );
    }

    fn assert_spawn_failed(result: Result<CommandResult, CommandCapError>) {
        assert!(
            matches!(result, Err(CommandCapError::SpawnFailed(_))),
            "expected SpawnFailed, got {result:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn guest_cwd_without_rule_cwd_is_denied() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let cap = pwd_cap(root.path(), None);

        assert_denied(cap.run("/bin/pwd", with_cwd(root.path())));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn guest_cwd_outside_rule_root_is_denied() {
        let root = tempfile::TempDir::new().expect("tempdir");
        projects(root.path());
        let elsewhere = root.path().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).expect("elsewhere");
        let cap = pwd_cap(root.path(), Some("${home}/projects"));

        assert_denied(cap.run("/bin/pwd", with_cwd(&elsewhere)));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn guest_cwd_escaping_with_dotdot_is_denied() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let projects = projects(root.path());
        std::fs::create_dir_all(root.path().join("home/elsewhere")).expect("elsewhere");
        let cap = pwd_cap(root.path(), Some("${home}/projects"));

        assert_denied(cap.run("/bin/pwd", with_cwd(&projects.join("../elsewhere"))));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn guest_cwd_with_dotdot_inside_root_is_denied() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let projects = projects(root.path());
        std::fs::create_dir_all(projects.join("repo")).expect("repo");
        let cap = pwd_cap(root.path(), Some("${home}/projects"));

        assert_denied(cap.run("/bin/pwd", with_cwd(&projects.join("repo/../repo"))));
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn guest_cwd_through_symlink_out_of_root_is_denied() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let projects = projects(root.path());
        let elsewhere = root.path().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).expect("elsewhere");
        std::os::unix::fs::symlink(&elsewhere, projects.join("link")).expect("symlink");
        let cap = pwd_cap(root.path(), Some("${home}/projects"));

        assert_denied(cap.run("/bin/pwd", with_cwd(&projects.join("link"))));
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn guest_cwd_through_dangling_symlink_is_denied() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let projects = projects(root.path());
        std::os::unix::fs::symlink(root.path().join("missing"), projects.join("link"))
            .expect("symlink");
        let cap = pwd_cap(root.path(), Some("${home}/projects"));

        assert_denied(cap.run("/bin/pwd", with_cwd(&projects.join("link"))));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn relative_guest_cwd_is_denied() {
        let root = tempfile::TempDir::new().expect("tempdir");
        projects(root.path());
        let cap = pwd_cap(root.path(), Some("${home}/projects"));

        assert_denied(cap.run("/bin/pwd", with_cwd(std::path::Path::new("."))));
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn rule_cwd_is_the_default_cwd() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let projects = projects(root.path());
        let cap = pwd_cap(root.path(), Some("${home}/projects"));

        let cwd = stdout_line(&cap.run("/bin/pwd", options(&[])).expect("pwd runs"));

        assert_eq!(
            std::path::Path::new(&cwd),
            projects.canonicalize().expect("canonical projects")
        );
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn missing_rule_cwd_root_is_created_for_the_default() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let cap = pwd_cap(root.path(), Some("${home}/projects"));

        let result = cap.run("/bin/pwd", options(&[]));

        assert!(result.is_ok(), "{result:?}");
        assert!(root.path().join("home/projects").is_dir());
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn guest_cwd_inside_rule_root_is_used() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let repo = projects(root.path()).join("repo");
        std::fs::create_dir_all(&repo).expect("repo");
        let cap = pwd_cap(root.path(), Some("${home}/projects"));

        let cwd = stdout_line(&cap.run("/bin/pwd", with_cwd(&repo)).expect("pwd runs"));

        assert_eq!(
            std::path::Path::new(&cwd),
            repo.canonicalize().expect("canonical repo")
        );
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn guest_cwd_equal_to_rule_root_is_used() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let projects = projects(root.path());
        let cap = pwd_cap(root.path(), Some("${home}/projects"));

        let cwd = stdout_line(&cap.run("/bin/pwd", with_cwd(&projects)).expect("pwd runs"));

        assert_eq!(
            std::path::Path::new(&cwd),
            projects.canonicalize().expect("canonical projects")
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn missing_guest_cwd_inside_root_fails_to_spawn() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let projects = projects(root.path());
        let cap = pwd_cap(root.path(), Some("${home}/projects"));

        assert_spawn_failed(cap.run("/bin/pwd", with_cwd(&projects.join("missing"))));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn guest_cwd_under_a_missing_rule_root_fails_to_spawn() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let cap = pwd_cap(root.path(), Some("${home}/projects"));

        assert_spawn_failed(cap.run(
            "/bin/pwd",
            with_cwd(&root.path().join("home/projects/repo")),
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_rule_root_that_cannot_be_created_fails_to_spawn() {
        let root = tempfile::TempDir::new().expect("tempdir");
        std::fs::create_dir_all(root.path().join("home")).expect("home");
        std::fs::write(root.path().join("home/projects"), b"a file").expect("file");
        let cap = pwd_cap(root.path(), Some("${home}/projects/repo"));

        assert_spawn_failed(cap.run("/bin/pwd", options(&[])));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn guest_cwd_that_is_a_file_fails_to_spawn() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let file = projects(root.path()).join("file");
        std::fs::write(&file, b"x").expect("file");
        let cap = pwd_cap(root.path(), Some("${home}/projects"));

        assert_spawn_failed(cap.run("/bin/pwd", with_cwd(&file)));
    }

    /// APFS volumes are case-insensitive by default, so a guest path in
    /// other case names the same directory and stays inside the root.
    #[cfg(target_os = "macos")]
    #[tokio::test(flavor = "multi_thread")]
    async fn guest_cwd_in_other_case_resolves_inside_root() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let repo = projects(root.path()).join("Repo");
        std::fs::create_dir_all(&repo).expect("repo");
        let other_case = repo.with_file_name("REPO");
        if !other_case.exists() {
            // A case-sensitive volume has no such directory.
            return;
        }
        let cap = pwd_cap(root.path(), Some("${home}/projects"));

        let cwd = stdout_line(
            &cap.run("/bin/pwd", with_cwd(&other_case))
                .expect("pwd runs"),
        );

        assert_eq!(
            std::path::Path::new(&cwd),
            repo.canonicalize().expect("canonical repo")
        );
    }

    // ─── awake, the gadget that ships with a command grant ─

    /// Paths for a gadget whose data directory follows the host's
    /// `<app data>/gadget-home/<id>/` layout below `root`.
    fn gadget_paths(root: &std::path::Path, gadget_id: &str) -> crate::paths::GadgetPaths {
        crate::paths::GadgetPaths {
            platform: std::sync::Arc::new(crate::paths::PlatformPaths {
                home: root.join("home"),
                xdg_config: root.join("home/Library/Application Support"),
                xdg_data: root.join("home/Library/Application Support"),
            }),
            gadget_data: root.join("app-data/gadget-home").join(gadget_id),
            gadget_archive: root
                .join("app-data/gadgets")
                .join(format!("{gadget_id}.torchsnap")),
        }
    }

    fn options(args: &[&str]) -> CommandOptions {
        CommandOptions {
            args: args.iter().map(|arg| arg.to_string()).collect(),
            cwd: None,
            env: vec![],
            stdin: None,
            timeout_ms: Some(5_000),
            max_output_bytes: None,
        }
    }

    /// The three scripts `gadgets/awake/src/backend/amphetamine.rs`
    /// sends, copied verbatim, so the host keeps accepting exactly
    /// what awake emits.
    const AWAKE_STATUS_SCRIPT: &str = "\
if application \"Amphetamine\" is running then\n\
tell application \"Amphetamine\"\n\
set isActive to session is active\n\
set remaining to session time remaining\n\
set displayAllowed to display sleep allowed\n\
end tell\n\
return isActive as text & \"|\" & remaining as text & \"|\" & displayAllowed as text\n\
else\n\
return \"not-running\"\n\
end if";
    const AWAKE_STOP_SCRIPT: &str = "tell application \"Amphetamine\" to end session";
    const AWAKE_START_SCRIPT: &str = "tell application \"Amphetamine\" to start new session \
         with options {duration:30, interval:minutes, displaySleepAllowed:false}";

    #[test]
    fn awake_manifest_compiles_into_a_command_cap() {
        let manifest = crate::wasm::manifest::Manifest::parse(include_str!(
            "../../../gadgets/awake/manifest.toml"
        ))
        .expect("awake's manifest parses");
        let rules = manifest
            .permissions
            .expect("awake declares permissions")
            .command;
        let root = tempfile::TempDir::new().expect("tempdir");
        let paths = gadget_paths(root.path(), "awake");

        let cap = CommandCap::new(&rules, &paths, paths.gadget_data.clone())
            .expect("awake's command rules compile");

        for script in [AWAKE_STATUS_SCRIPT, AWAKE_STOP_SCRIPT, AWAKE_START_SCRIPT] {
            argv_matcher::matches(
                &cap.rules,
                "osascript",
                &[String::from("-e"), script.into()],
            )
            .unwrap_or_else(|e| panic!("awake's rule rejects {script:?}: {e}"));
        }
        let altered = AWAKE_STOP_SCRIPT.replace("end session", "quit");
        assert!(
            argv_matcher::matches(&cap.rules, "osascript", &["-e".into(), altered]).is_err(),
            "awake's rule must not accept other scripts"
        );
    }

    /// Awake names its binary without a path, so this pins that a bare
    /// name still resolves and `osascript` runs in the environment the
    /// host gives commands. The script touches no other application.
    #[cfg(target_os = "macos")]
    #[tokio::test(flavor = "multi_thread")]
    async fn osascript_runs_by_name_in_the_command_environment() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let paths = gadget_paths(root.path(), "awake");
        let rule = CommandPermissionDef {
            binary: "osascript".into(),
            argv: vec![
                crate::wasm::manifest::ArgvConstraint::Literal { value: "-e".into() },
                crate::wasm::manifest::ArgvConstraint::Literal {
                    value: "return \"ok\"".into(),
                },
            ],
            cwd: None,
            timeout_ms_max: None,
            max_output_bytes: None,
            max_stdin_bytes: None,
            env: Default::default(),
        };
        let cap = CommandCap::new(&[rule], &paths, paths.gadget_data.clone()).expect("compile");

        let result = cap
            .run("osascript", options(&["-e", "return \"ok\""]))
            .expect("osascript runs");

        assert_eq!(
            result.exit_code,
            Some(0),
            "stderr: {:?}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(result.stdout, b"ok\n");
    }
}
