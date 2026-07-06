// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// ClipboardCap
//
// Write-only clipboard capability. The writer closure wraps
// the platform clipboard API. Read access is intentionally
// not exposed — see the `clipboard` interface doc in the
// WIT file.
// =========================================================

#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    #[error("clipboard backend failure: {0}")]
    BackendFailure(String),
}

/// Platform clipboard write function: takes the text to write,
/// returns a backend error message on failure.
type ClipboardWriter = Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

pub struct ClipboardCap {
    writer: ClipboardWriter,
}

impl ClipboardCap {
    pub fn new(writer: ClipboardWriter) -> Self {
        Self { writer }
    }

    pub fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
        (self.writer)(text).map_err(ClipboardError::BackendFailure)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn write_text_calls_through_to_writer() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&call_count);
        let cap = ClipboardCap::new(Box::new(move |text| {
            counter.fetch_add(1, Ordering::Relaxed);
            assert_eq!(text, "hello");
            Ok(())
        }));
        cap.write_text("hello").expect("should succeed");
        assert_eq!(call_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn write_text_propagates_writer_error() {
        let cap = ClipboardCap::new(Box::new(|_| Err("backend broke".into())));
        let err = cap.write_text("anything").unwrap_err();
        assert!(matches!(err, ClipboardError::BackendFailure(_)));
        assert!(err.to_string().contains("backend broke"));
    }

    #[test]
    fn error_display_includes_message() {
        let err = ClipboardError::BackendFailure("test error".into());
        assert!(err.to_string().contains("test error"));
    }
}
