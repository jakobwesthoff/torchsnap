// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Plugin Asset Protocol
//
// Registers a `torchsnap-plugin://` custom URI scheme that
// serves frontend assets directly from plugin sources. This
// avoids extracting files to disk — the protocol handler
// reads from `PluginSource::read_file()` on demand.
//
// URI format:
//   torchsnap-plugin://localhost/<plugin-id>/<file-path>
//
// On macOS/Linux the scheme is `torchsnap-plugin://`.
// On Windows Tauri maps it to `http://torchsnap-plugin.localhost/`.
// We parse from the `http::Request` path, so both shapes work.
// =========================================================

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

use tauri::http;

use super::source::PluginSource;

// =========================================================
// Plugin Source Registry
// =========================================================

/// Shared registry mapping plugin IDs to their sources.
/// Populated during plugin loading, read by the protocol
/// handler on every asset request.
pub type PluginSourceRegistry = Arc<RwLock<HashMap<String, Arc<dyn PluginSource>>>>;

/// Create an empty registry.
pub fn new_registry() -> PluginSourceRegistry {
    Arc::new(RwLock::new(HashMap::new()))
}

// =========================================================
// Protocol Registration
// =========================================================

/// Register the `torchsnap-plugin` URI scheme on the Tauri
/// builder. Must be called before `.build()` / `.setup()`
/// because custom schemes are bound to the webview at
/// creation time.
pub fn register_plugin_protocol<R: tauri::Runtime>(
    builder: tauri::Builder<R>,
    registry: PluginSourceRegistry,
) -> tauri::Builder<R> {
    builder.register_asynchronous_uri_scheme_protocol(
        "torchsnap-plugin",
        move |_ctx, request, responder| {
            let registry = Arc::clone(&registry);

            // Spawn blocking because PluginSource::read_file may
            // hold a Mutex (ArchiveSource) and do file I/O.
            std::thread::spawn(move || {
                let response = handle_request(&registry, &request);
                responder.respond(response);
            });
        },
    )
}

// =========================================================
// Request Handler
// =========================================================

/// Parse the request, look up the plugin source, read the
/// file, and build the HTTP response.
fn handle_request(
    registry: &PluginSourceRegistry,
    request: &http::Request<Vec<u8>>,
) -> http::Response<Vec<u8>> {
    // Extract the origin for CORS. Echo the request's Origin
    // header so it works in both dev (http://localhost:1420)
    // and prod (tauri://localhost, https://tauri.localhost).
    let origin = request
        .headers()
        .get("Origin")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("*");

    match serve_plugin_asset(registry, request) {
        Ok((body, content_type)) => http::Response::builder()
            .status(200)
            .header("Content-Type", content_type)
            .header("Access-Control-Allow-Origin", origin)
            .body(body)
            .expect("valid response"),
        Err(ErrorResponse { status, message }) => http::Response::builder()
            .status(status)
            .header("Content-Type", "text/plain; charset=utf-8")
            .header("Access-Control-Allow-Origin", origin)
            .body(message.into_bytes())
            .expect("valid error response"),
    }
}

struct ErrorResponse {
    status: u16,
    message: String,
}

/// Core logic: parse URI, look up source, read file, detect
/// content type.
fn serve_plugin_asset(
    registry: &PluginSourceRegistry,
    request: &http::Request<Vec<u8>>,
) -> Result<(Vec<u8>, String), ErrorResponse> {
    let path = request.uri().path();

    // Strip leading slash and split into plugin-id / file-path.
    let trimmed = path.trim_start_matches('/');
    let (plugin_id, file_path) = trimmed.split_once('/').ok_or_else(|| ErrorResponse {
        status: 400,
        message: format!("invalid path: expected /<plugin-id>/<file-path>, got {path}"),
    })?;

    if plugin_id.is_empty() {
        return Err(ErrorResponse {
            status: 400,
            message: "missing plugin ID in path".to_string(),
        });
    }

    if file_path.is_empty() {
        return Err(ErrorResponse {
            status: 400,
            message: "missing file path after plugin ID".to_string(),
        });
    }

    // Look up the plugin source.
    let sources = registry.read().expect("registry not poisoned");
    let source = sources.get(plugin_id).ok_or_else(|| ErrorResponse {
        status: 404,
        message: format!("unknown plugin: {plugin_id}"),
    })?;

    // Read the file from the plugin source.
    let data = source.read_file(file_path).map_err(|e| ErrorResponse {
        status: 404,
        message: format!("{e:#}"),
    })?;

    let content_type = detect_content_type(file_path, &data);
    Ok((data, content_type))
}

// =========================================================
// Content-Type Detection
// =========================================================

/// Detect the content type for a plugin asset. Uses the
/// `infer` crate for binary formats (images, wasm, etc.)
/// and falls back to extension-based detection for text
/// formats that `infer` can't identify by magic bytes.
fn detect_content_type(path: &str, data: &[u8]) -> String {
    // Try magic-byte detection first (works for images, wasm, etc.).
    if let Some(kind) = infer::get(data) {
        return kind.mime_type().to_string();
    }

    // Fall back to file extension for text-based formats that
    // infer can't detect by content (JS, CSS, HTML, JSON, etc.).
    match Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
    {
        Some("js" | "mjs") => "text/javascript",
        Some("css") => "text/css",
        Some("html" | "htm") => "text/html",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("wasm") => "application/wasm",
        Some("txt") => "text/plain",
        _ => "application/octet-stream",
    }
    .to_string()
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm::manifest::Manifest;
    use crate::wasm::source::PluginSource;

    // =====================================================
    // Test helpers
    // =====================================================

    /// In-memory plugin source for testing. Stores files as
    /// a simple hashmap — no filesystem or zip involved.
    struct MemorySource {
        manifest: Manifest,
        files: HashMap<String, Vec<u8>>,
    }

    impl PluginSource for MemorySource {
        fn manifest(&self) -> &Manifest {
            &self.manifest
        }

        fn read_file(&self, path: &str) -> anyhow::Result<Vec<u8>> {
            self.files
                .get(path)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("file not found: {path}"))
        }
    }

    fn test_manifest() -> Manifest {
        Manifest::parse(
            r#"
            [plugin]
            id = "test-plugin"
            name = "Test Plugin"
            description = "A test plugin"
            version = "0.1.0"
            wasm = "test.wasm"
            icon = "heroicons:beaker"
        "#,
        )
        .expect("valid manifest")
    }

    fn test_registry(files: HashMap<String, Vec<u8>>) -> PluginSourceRegistry {
        let registry = new_registry();
        let source = Arc::new(MemorySource {
            manifest: test_manifest(),
            files,
        });
        registry
            .write()
            .expect("lock")
            .insert("test-plugin".to_string(), source);
        registry
    }

    fn make_request(path: &str) -> http::Request<Vec<u8>> {
        make_request_with_origin(path, "tauri://localhost")
    }

    fn make_request_with_origin(path: &str, origin: &str) -> http::Request<Vec<u8>> {
        http::Request::builder()
            .uri(format!("torchsnap-plugin://localhost{path}"))
            .header("Origin", origin)
            .body(vec![])
            .expect("valid request")
    }

    // =====================================================
    // Expected cases
    // =====================================================

    #[test]
    fn serve_js_file() {
        let mut files = HashMap::new();
        files.insert(
            "frontend/launcher.js".to_string(),
            b"export function Echo() {}".to_vec(),
        );
        let registry = test_registry(files);
        let request = make_request("/test-plugin/frontend/launcher.js");

        let response = handle_request(&registry, &request);
        assert_eq!(response.status(), 200);
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "text/javascript"
        );
        assert_eq!(response.body(), b"export function Echo() {}");
    }

    #[test]
    fn serve_css_file() {
        let mut files = HashMap::new();
        files.insert(
            "frontend/styles.css".to_string(),
            b".echo { color: red; }".to_vec(),
        );
        let registry = test_registry(files);
        let request = make_request("/test-plugin/frontend/styles.css");

        let response = handle_request(&registry, &request);
        assert_eq!(response.status(), 200);
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "text/css"
        );
    }

    #[test]
    fn serve_json_file() {
        let mut files = HashMap::new();
        files.insert("data.json".to_string(), b"{}".to_vec());
        let registry = test_registry(files);
        let request = make_request("/test-plugin/data.json");

        let response = handle_request(&registry, &request);
        assert_eq!(response.status(), 200);
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "application/json"
        );
    }

    #[test]
    fn serve_binary_png() {
        // Minimal PNG header (8-byte magic).
        let png_header = [
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG magic
            0x00, 0x00, 0x00, 0x0D, // IHDR length
            0x49, 0x48, 0x44, 0x52, // IHDR chunk
        ];
        let mut files = HashMap::new();
        files.insert("icon.png".to_string(), png_header.to_vec());
        let registry = test_registry(files);
        let request = make_request("/test-plugin/icon.png");

        let response = handle_request(&registry, &request);
        assert_eq!(response.status(), 200);
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "image/png"
        );
    }

    #[test]
    fn cors_header_echoed() {
        let mut files = HashMap::new();
        files.insert("test.js".to_string(), b"//js".to_vec());
        let registry = test_registry(files);

        // Dev origin
        let request = make_request_with_origin("/test-plugin/test.js", "http://localhost:1420");
        let response = handle_request(&registry, &request);
        assert_eq!(
            response.headers().get("Access-Control-Allow-Origin").unwrap(),
            "http://localhost:1420"
        );

        // Prod origin
        let request = make_request_with_origin("/test-plugin/test.js", "tauri://localhost");
        let response = handle_request(&registry, &request);
        assert_eq!(
            response.headers().get("Access-Control-Allow-Origin").unwrap(),
            "tauri://localhost"
        );
    }

    #[test]
    fn serve_file_preserves_bytes() {
        // Verify all 256 byte values round-trip correctly.
        let all_bytes: Vec<u8> = (0..=255).collect();
        let mut files = HashMap::new();
        files.insert("binary.dat".to_string(), all_bytes.clone());
        let registry = test_registry(files);
        let request = make_request("/test-plugin/binary.dat");

        let response = handle_request(&registry, &request);
        assert_eq!(response.status(), 200);
        assert_eq!(response.body(), &all_bytes);
    }

    // =====================================================
    // Error cases
    // =====================================================

    #[test]
    fn unknown_plugin_404() {
        let registry = test_registry(HashMap::new());
        let request = make_request("/nonexistent-plugin/file.js");

        let response = handle_request(&registry, &request);
        assert_eq!(response.status(), 404);
        let body = String::from_utf8_lossy(response.body());
        assert!(body.contains("unknown plugin"), "body: {body}");
    }

    #[test]
    fn missing_file_404() {
        let registry = test_registry(HashMap::new());
        let request = make_request("/test-plugin/nonexistent.js");

        let response = handle_request(&registry, &request);
        assert_eq!(response.status(), 404);
    }

    #[test]
    fn path_traversal_400_or_404() {
        let registry = test_registry(HashMap::new());
        let request = make_request("/test-plugin/../../../etc/passwd");

        let response = handle_request(&registry, &request);
        // Depending on PluginSource implementation, this may be
        // a 404 (path rejected by source) — either way, no data
        // should be returned.
        assert!(
            response.status() == 400 || response.status() == 404,
            "expected 4xx, got {}",
            response.status()
        );
    }

    #[test]
    fn empty_file_path_400() {
        let registry = test_registry(HashMap::new());
        // Path is just "/test-plugin/" with no file path.
        let request = make_request("/test-plugin/");

        let response = handle_request(&registry, &request);
        assert_eq!(response.status(), 400);
        let body = String::from_utf8_lossy(response.body());
        assert!(body.contains("missing file path"), "body: {body}");
    }

    #[test]
    fn no_plugin_id_400() {
        let registry = test_registry(HashMap::new());
        let request = make_request("/");

        let response = handle_request(&registry, &request);
        assert_eq!(response.status(), 400);
    }

    #[test]
    fn bare_root_path_400() {
        let registry = test_registry(HashMap::new());
        // No path segments at all after scheme.
        let request = http::Request::builder()
            .uri("torchsnap-plugin://localhost")
            .header("Origin", "tauri://localhost")
            .body(vec![])
            .expect("valid request");

        let response = handle_request(&registry, &request);
        assert_eq!(response.status(), 400);
    }

    // =====================================================
    // Edge cases
    // =====================================================

    #[test]
    fn unknown_extension_falls_back_to_octet_stream() {
        let mut files = HashMap::new();
        files.insert("data.xyz".to_string(), b"something".to_vec());
        let registry = test_registry(files);
        let request = make_request("/test-plugin/data.xyz");

        let response = handle_request(&registry, &request);
        assert_eq!(response.status(), 200);
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "application/octet-stream"
        );
    }

    #[test]
    fn nested_file_path() {
        let mut files = HashMap::new();
        files.insert(
            "assets/images/logo.svg".to_string(),
            b"<svg></svg>".to_vec(),
        );
        let registry = test_registry(files);
        let request = make_request("/test-plugin/assets/images/logo.svg");

        let response = handle_request(&registry, &request);
        assert_eq!(response.status(), 200);
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "image/svg+xml"
        );
    }

    #[test]
    fn multiple_plugins_in_registry() {
        let registry = new_registry();

        let source_a = Arc::new(MemorySource {
            manifest: Manifest::parse(
                r#"
                [plugin]
                id = "plugin-a"
                name = "A"
                description = "A"
                version = "0.1.0"
                wasm = "a.wasm"
                icon = "heroicons:beaker"
            "#,
            )
            .unwrap(),
            files: HashMap::from([("file.js".to_string(), b"// plugin A".to_vec())]),
        });

        let source_b = Arc::new(MemorySource {
            manifest: Manifest::parse(
                r#"
                [plugin]
                id = "plugin-b"
                name = "B"
                description = "B"
                version = "0.1.0"
                wasm = "b.wasm"
                icon = "heroicons:beaker"
            "#,
            )
            .unwrap(),
            files: HashMap::from([("file.js".to_string(), b"// plugin B".to_vec())]),
        });

        {
            let mut sources = registry.write().expect("lock");
            sources.insert("plugin-a".to_string(), source_a);
            sources.insert("plugin-b".to_string(), source_b);
        }

        let resp_a = handle_request(&registry, &make_request("/plugin-a/file.js"));
        let resp_b = handle_request(&registry, &make_request("/plugin-b/file.js"));

        assert_eq!(resp_a.body(), b"// plugin A");
        assert_eq!(resp_b.body(), b"// plugin B");
    }

    // =====================================================
    // Content-type detection
    // =====================================================

    #[test]
    fn detect_js_by_extension() {
        assert_eq!(detect_content_type("bundle.js", b"//code"), "text/javascript");
        assert_eq!(detect_content_type("module.mjs", b"//code"), "text/javascript");
    }

    #[test]
    fn detect_css_by_extension() {
        assert_eq!(detect_content_type("styles.css", b".x{}"), "text/css");
    }

    #[test]
    fn detect_html_by_extension() {
        assert_eq!(detect_content_type("page.html", b"<html>"), "text/html");
        assert_eq!(detect_content_type("page.htm", b"<html>"), "text/html");
    }

    #[test]
    fn detect_json_by_extension() {
        assert_eq!(detect_content_type("data.json", b"{}"), "application/json");
    }

    #[test]
    fn detect_svg_by_extension() {
        assert_eq!(
            detect_content_type("icon.svg", b"<svg>"),
            "image/svg+xml"
        );
    }

    #[test]
    fn detect_wasm_by_extension() {
        // \0asm magic not present, fallback to extension.
        assert_eq!(
            detect_content_type("module.wasm", b"not real wasm"),
            "application/wasm"
        );
    }

    #[test]
    fn fallback_for_unknown_extension() {
        assert_eq!(
            detect_content_type("data.xyz", b"something"),
            "application/octet-stream"
        );
    }

    // =====================================================
    // Integration tests with real DirectorySource
    // =====================================================

    use crate::wasm::source::DirectorySource;

    const FRONTEND_MANIFEST: &str = r#"
        [plugin]
        id = "frontend-test"
        name = "Frontend Test"
        description = "Test plugin with frontend assets"
        version = "0.1.0"
        wasm = "test.wasm"
        icon = "heroicons:beaker"
        prefixes = ["!"]

        [frontend]
        launcher-bundle = "frontend/launcher.js"
        launcher-css = "frontend/launcher.css"

        [frontend.views]
        echo = "Echo"
    "#;

    /// Create a temporary plugin directory with manifest and
    /// frontend assets, register it via DirectorySource.
    fn directory_registry(
        manifest: &str,
        files: &[(&str, &[u8])],
    ) -> (tempfile::TempDir, PluginSourceRegistry) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let root = dir.path();

        std::fs::write(root.join("manifest.toml"), manifest)
            .expect("write manifest");

        for (path, contents) in files {
            let full = root.join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).expect("create parent dirs");
            }
            std::fs::write(&full, contents).expect("write file");
        }

        let source = Arc::new(
            DirectorySource::open(root).expect("open directory source"),
        );
        let plugin_id = source.manifest().plugin.id.to_string();

        let registry = new_registry();
        registry
            .write()
            .expect("lock")
            .insert(plugin_id, source);

        (dir, registry)
    }

    #[test]
    fn directory_source_serves_js() {
        let js = b"export function Echo() { return null; }";
        let (_dir, registry) = directory_registry(
            FRONTEND_MANIFEST,
            &[
                ("test.wasm", b"\0asm"),
                ("frontend/launcher.js", js),
                ("frontend/launcher.css", b".echo {}"),
            ],
        );

        let request = make_request("/frontend-test/frontend/launcher.js");
        let response = handle_request(&registry, &request);

        assert_eq!(response.status(), 200);
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "text/javascript",
        );
        assert_eq!(response.body(), js);
    }

    #[test]
    fn directory_source_serves_css() {
        let css = b".echo-view { padding: 1rem; }";
        let (_dir, registry) = directory_registry(
            FRONTEND_MANIFEST,
            &[
                ("test.wasm", b"\0asm"),
                ("frontend/launcher.js", b"//js"),
                ("frontend/launcher.css", css),
            ],
        );

        let request = make_request("/frontend-test/frontend/launcher.css");
        let response = handle_request(&registry, &request);

        assert_eq!(response.status(), 200);
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "text/css",
        );
        assert_eq!(response.body(), css);
    }

    #[test]
    fn directory_source_rejects_traversal() {
        let (_dir, registry) = directory_registry(
            FRONTEND_MANIFEST,
            &[
                ("test.wasm", b"\0asm"),
                ("frontend/launcher.js", b"//js"),
                ("frontend/launcher.css", b"/*css*/"),
            ],
        );

        let request = make_request("/frontend-test/../../../etc/passwd");
        let response = handle_request(&registry, &request);

        assert!(
            response.status() == 400 || response.status() == 404,
            "expected 4xx, got {}",
            response.status()
        );
    }

    #[test]
    fn directory_source_missing_file_404() {
        let (_dir, registry) = directory_registry(
            FRONTEND_MANIFEST,
            &[
                ("test.wasm", b"\0asm"),
                ("frontend/launcher.js", b"//js"),
                ("frontend/launcher.css", b"/*css*/"),
            ],
        );

        let request = make_request("/frontend-test/frontend/nonexistent.js");
        let response = handle_request(&registry, &request);
        assert_eq!(response.status(), 404);
    }

    #[test]
    fn hello_world_manifest_with_frontend_parses() {
        let toml = r#"
            [plugin]
            id = "hello-world"
            name = "Hello World"
            description = "Test plugin"
            version = "0.1.0"
            wasm = "hello_world_plugin.wasm"
            icon = "heroicons:hand-raised"
            prefixes = ["!"]

            [frontend]
            launcher-bundle = "frontend/launcher.js"
            launcher-css = "frontend/launcher.css"

            [frontend.views]
            echo = "Echo"
        "#;

        let m = Manifest::parse(toml).expect("should parse");
        assert_eq!(m.plugin.prefixes, vec!["!"]);

        let fe = m.frontend.as_ref().expect("frontend");
        assert_eq!(fe.launcher_bundle.as_deref(), Some("frontend/launcher.js"));
        assert_eq!(fe.launcher_css.as_deref(), Some("frontend/launcher.css"));
        assert_eq!(fe.views.get("echo").map(String::as_str), Some("Echo"));
    }
}
