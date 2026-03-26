# Cleanup: Replace fully-qualified paths with imports

Audit all Rust source files for unnecessarily fully-qualified paths
(e.g., `std::process::Command`, `std::thread::spawn`,
`std::time::SystemTime`) and replace them with proper `use` imports
at the top of the file.

## Scope

- Check all files under `src-tauri/src/`
- Focus on `std::` paths that are used more than once or could be
  cleaner as imports
- Also check for inconsistent import style within individual files
