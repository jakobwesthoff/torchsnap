// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Shared test fixtures for manifest unit tests.

/// Build a minimal valid manifest TOML, optionally appending extra
/// sections. Used by every manifest sub-module's test suite to avoid
/// repeating the full required-field boilerplate.
pub(crate) fn minimal(extra: &str) -> String {
    format!(
        r#"
        [gadget]
        id = "test-gadget"
        name = "Test Gadget"
        description = "A test gadget"
        version = "0.1.0"
        wasm = "test.wasm"
        icon = "heroicons:beaker"
        {extra}
        "#
    )
}

// =========================================================
// `.torchsnap` archive fixtures
//
// Install, staging and source tests need real archives on disk.
// Each builder writes the zip into its own temp dir and returns that
// dir alongside the path; the file lives as long as the `TempDir`.
// =========================================================

/// Write a `.torchsnap` archive with the given `manifest.toml` and
/// extra entries.
pub(crate) fn write_archive(
    manifest_toml: &str,
    files: &[(&str, &[u8])],
) -> (tempfile::TempDir, std::path::PathBuf) {
    let mut entries = vec![("manifest.toml", manifest_toml.as_bytes())];
    entries.extend_from_slice(files);
    write_archive_without_manifest(&entries)
}

/// Write a zip archive with exactly the given entries, which lets a
/// test leave out `manifest.toml`.
pub(crate) fn write_archive_without_manifest(
    files: &[(&str, &[u8])],
) -> (tempfile::TempDir, std::path::PathBuf) {
    use std::io::{Cursor, Write as _};
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    let mut buf = Cursor::new(Vec::new());
    {
        let mut writer = ZipWriter::new(&mut buf);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        for (path, contents) in files {
            writer.start_file(*path, options).expect("start file entry");
            writer.write_all(contents).expect("write file");
        }

        writer.finish().expect("finalize zip");
    }

    let dir = tempfile::tempdir().expect("create temp dir");
    let archive_path = dir.path().join("gadget.torchsnap");
    std::fs::write(&archive_path, buf.into_inner()).expect("write archive");

    (dir, archive_path)
}

/// Write a loadable gadget archive with the given id and version,
/// `extra_toml` appended to the manifest (typically a `[permissions]`
/// or `[storage]` section) and a placeholder WASM entry.
pub(crate) fn archive_with_manifest(
    id: &str,
    version: &str,
    extra_toml: &str,
) -> (tempfile::TempDir, std::path::PathBuf) {
    let manifest = format!(
        r#"
        [gadget]
        id = "{id}"
        name = "Gadget {id}"
        description = "Test gadget {id}"
        version = "{version}"
        wasm = "gadget.wasm"
        icon = "heroicons:beaker"
        {extra_toml}
        "#
    );
    write_archive(&manifest, &[("gadget.wasm", b"fake wasm")])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm::source::{ArchiveSource, GadgetSource};

    #[test]
    fn archive_with_manifest_opens_with_the_given_identity() {
        let (_dir, path) = archive_with_manifest("weather", "1.4.0", "");

        let source = ArchiveSource::open(&path).expect("archive should open");
        let gadget = &source.manifest().gadget;
        assert_eq!(gadget.id.as_str(), "weather");
        assert_eq!(gadget.version, "1.4.0");
        assert_eq!(
            source.read_wasm().expect("wasm entry should exist"),
            b"fake wasm"
        );
    }

    #[test]
    fn archive_with_manifest_appends_extra_sections() {
        let (_dir, path) = archive_with_manifest(
            "weather",
            "1.4.0",
            r#"
            [permissions.http]
            origins = ["https://api.example.com"]
            "#,
        );

        let source = ArchiveSource::open(&path).expect("archive should open");
        let permissions = source
            .manifest()
            .permissions
            .as_ref()
            .expect("permissions section should parse");
        assert_eq!(
            permissions.http.as_ref().expect("http section").origins,
            vec!["https://api.example.com"]
        );
    }

    #[test]
    fn write_archive_without_manifest_has_no_manifest_entry() {
        let (_dir, path) = write_archive_without_manifest(&[("gadget.wasm", b"wasm")]);

        assert!(ArchiveSource::open(&path).is_err());
    }
}
