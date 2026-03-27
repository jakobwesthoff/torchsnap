// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// osascript Helper
//
// Thin wrapper around the macOS `osascript` command-line tool
// for running AppleScript expressions. Used by system commands
// and any other macOS-specific code that needs to interact
// with the system via AppleScript.
// =========================================================

use std::process::Command;

use anyhow::Context;

/// Run an AppleScript expression and return its trimmed stdout.
pub fn eval(script: &str) -> anyhow::Result<String> {
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .context("spawn osascript process")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("osascript failed: {}", stderr.trim());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Run an AppleScript expression, discarding output.
pub fn run(script: &str) -> anyhow::Result<()> {
    eval(script)?;
    Ok(())
}
