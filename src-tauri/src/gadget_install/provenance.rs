// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Where a downloaded archive came from, for the install review.
//!
//! macOS browsers tag downloads with two extended attributes:
//! `com.apple.metadata:kMDItemWhereFroms`, a binary plist array holding
//! the download URL and usually the referring page, and
//! `com.apple.quarantine`, a `flags;timestamp;agent;uuid` string that
//! names the downloading app. Both are optional hints: a file copied
//! without its attributes, or created locally, simply has none, and
//! the review then shows no provenance line. Other platforms never
//! report provenance.

use serde::Serialize;

const WHERE_FROMS_ATTRIBUTE: &str = "com.apple.metadata:kMDItemWhereFroms";
const QUARANTINE_ATTRIBUTE: &str = "com.apple.quarantine";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Provenance {
    pub download_url: Option<String>,
    pub referrer_url: Option<String>,
    /// The app that downloaded the file, e.g. `Safari`.
    pub downloaded_by: Option<String>,
}

/// The non-empty strings of a `kMDItemWhereFroms` value, in order.
/// Anything that is not a plist array yields nothing.
pub fn parse_where_froms(bytes: &[u8]) -> Vec<String> {
    match plist::Value::from_reader(std::io::Cursor::new(bytes)) {
        Ok(plist::Value::Array(values)) => values
            .into_iter()
            .filter_map(|value| value.into_string())
            .filter(|url| !url.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

/// The agent field of a `com.apple.quarantine` value.
pub fn parse_quarantine(bytes: &[u8]) -> Option<String> {
    let value = std::str::from_utf8(bytes).ok()?;
    let agent = value.split(';').nth(2)?.trim();
    (!agent.is_empty()).then(|| agent.to_string())
}

/// Read both attributes of `path`. `None` when neither says anything.
// TODO(install-queue): staging reads provenance for every request
// (plan step 10); until then only the tests call this.
#[cfg_attr(not(test), expect(dead_code, reason = "consumed by the install queue"))]
pub fn read_provenance(path: &std::path::Path) -> Option<Provenance> {
    let mut urls = read_attribute(path, WHERE_FROMS_ATTRIBUTE)
        .map(|bytes| parse_where_froms(&bytes))
        .unwrap_or_default()
        .into_iter();
    let provenance = Provenance {
        download_url: urls.next(),
        referrer_url: urls.next(),
        downloaded_by: read_attribute(path, QUARANTINE_ATTRIBUTE)
            .and_then(|bytes| parse_quarantine(&bytes)),
    };
    let known = provenance.download_url.is_some()
        || provenance.referrer_url.is_some()
        || provenance.downloaded_by.is_some();
    known.then_some(provenance)
}

/// The raw value of extended attribute `name`, or `None` when the file
/// or the attribute is missing or unreadable.
#[cfg(target_os = "macos")]
fn read_attribute(path: &std::path::Path, name: &str) -> Option<Vec<u8>> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt as _;

    let path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let name = CString::new(name).ok()?;

    // The attribute can change size between the probe and the read
    // (`ERANGE`), so the pair is retried a few times before giving up.
    for _ in 0..3 {
        // SAFETY: both strings are NUL-terminated and live for the
        // call; a null buffer with size 0 asks only for the length.
        let size =
            unsafe { libc::getxattr(path.as_ptr(), name.as_ptr(), std::ptr::null_mut(), 0, 0, 0) };
        if size < 0 {
            return None;
        }
        let mut buffer = vec![0u8; size as usize];
        // SAFETY: `buffer` has exactly `buffer.len()` writable bytes.
        let read = unsafe {
            libc::getxattr(
                path.as_ptr(),
                name.as_ptr(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                0,
                0,
            )
        };
        if read >= 0 {
            buffer.truncate(read as usize);
            return Some(buffer);
        }
        if std::io::Error::last_os_error().raw_os_error() != Some(libc::ERANGE) {
            return None;
        }
    }
    None
}

#[cfg(not(target_os = "macos"))]
fn read_attribute(_path: &std::path::Path, _name: &str) -> Option<Vec<u8>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binary_plist(value: plist::Value) -> Vec<u8> {
        let mut bytes = Vec::new();
        value
            .to_writer_binary(&mut bytes)
            .expect("plist serializes");
        bytes
    }

    fn where_froms(urls: &[&str]) -> Vec<u8> {
        binary_plist(plist::Value::Array(
            urls.iter()
                .map(|url| plist::Value::String(url.to_string()))
                .collect(),
        ))
    }

    // =========================================================
    // kMDItemWhereFroms
    // =========================================================

    #[test]
    fn where_froms_yields_download_and_referrer_urls() {
        let urls = parse_where_froms(&where_froms(&[
            "https://github.com/acme/weather/releases/download/v1/weather.torchsnap",
            "https://github.com/acme/weather/releases",
        ]));

        assert_eq!(
            urls,
            vec![
                "https://github.com/acme/weather/releases/download/v1/weather.torchsnap",
                "https://github.com/acme/weather/releases",
            ]
        );
    }

    #[test]
    fn where_froms_skips_empty_and_non_string_entries() {
        let urls = parse_where_froms(&binary_plist(plist::Value::Array(vec![
            plist::Value::String(String::new()),
            plist::Value::Integer(7.into()),
            plist::Value::String("https://example.com/a.torchsnap".to_string()),
        ])));

        assert_eq!(urls, vec!["https://example.com/a.torchsnap"]);
    }

    #[test]
    fn where_froms_of_the_wrong_plist_type_or_garbage_is_empty() {
        assert!(parse_where_froms(&binary_plist(plist::Value::String("x".into()))).is_empty());
        assert!(parse_where_froms(b"not a plist").is_empty());
        assert!(parse_where_froms(b"").is_empty());
    }

    // =========================================================
    // com.apple.quarantine
    // =========================================================

    #[test]
    fn quarantine_yields_the_downloading_agent() {
        assert_eq!(
            parse_quarantine(b"0083;66f2a1b0;Safari;8C1D2F4E-0000-0000-0000-000000000000"),
            Some("Safari".to_string())
        );
    }

    #[test]
    fn quarantine_without_an_agent_or_malformed_is_none() {
        assert_eq!(parse_quarantine(b"0083;66f2a1b0;;"), None);
        assert_eq!(parse_quarantine(b"0083"), None);
        assert_eq!(parse_quarantine(&[0xff, 0xfe]), None);
    }

    // =========================================================
    // Reading from a file
    // =========================================================

    #[test]
    fn a_file_without_attributes_has_no_provenance() {
        let file = tempfile::NamedTempFile::new().expect("create temp file");

        assert_eq!(read_provenance(file.path()), None);
    }

    #[test]
    fn a_missing_file_has_no_provenance() {
        assert_eq!(
            read_provenance(std::path::Path::new("/nonexistent/x.torchsnap")),
            None
        );
    }

    #[cfg(target_os = "macos")]
    fn set_xattr(path: &std::path::Path, name: &str, value: &[u8]) {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt as _;

        let path = CString::new(path.as_os_str().as_bytes()).expect("path has no NUL");
        let name = CString::new(name).expect("name has no NUL");
        // SAFETY: both strings are NUL-terminated and outlive the call;
        // `value` points to `value.len()` readable bytes.
        let result = unsafe {
            libc::setxattr(
                path.as_ptr(),
                name.as_ptr(),
                value.as_ptr().cast(),
                value.len(),
                0,
                0,
            )
        };
        assert_eq!(result, 0, "setxattr failed");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_downloaded_file_reports_where_it_came_from() {
        let file = tempfile::NamedTempFile::new().expect("create temp file");
        set_xattr(
            file.path(),
            WHERE_FROMS_ATTRIBUTE,
            &where_froms(&[
                "https://github.com/acme/weather/releases/download/v1/weather.torchsnap",
                "https://github.com/acme/weather/releases",
            ]),
        );
        set_xattr(
            file.path(),
            QUARANTINE_ATTRIBUTE,
            b"0083;66f2a1b0;Safari;8C1D2F4E-0000-0000-0000-000000000000",
        );

        assert_eq!(
            read_provenance(file.path()),
            Some(Provenance {
                download_url: Some(
                    "https://github.com/acme/weather/releases/download/v1/weather.torchsnap"
                        .to_string()
                ),
                referrer_url: Some("https://github.com/acme/weather/releases".to_string()),
                downloaded_by: Some("Safari".to_string()),
            })
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_large_attribute_is_read_completely() {
        let file = tempfile::NamedTempFile::new().expect("create temp file");
        let long_url = format!("https://example.com/{}", "a".repeat(4000));
        set_xattr(
            file.path(),
            WHERE_FROMS_ATTRIBUTE,
            &where_froms(&[&long_url]),
        );

        let provenance = read_provenance(file.path()).expect("provenance present");

        assert_eq!(provenance.download_url, Some(long_url));
    }
}
