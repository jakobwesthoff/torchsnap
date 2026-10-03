// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// macOS System Commands Factory
//
// Assembles the full set of system commands available on macOS.
// =========================================================

mod appearance;
mod power;
mod utilities;

use std::process::Command;

use anyhow::Context;

use super::SystemCommand;

/// Run a system tool to completion and fail on a non-zero exit, with the
/// tool's stderr in the error, so `execute` does not dismiss the
/// launcher as if the command worked. `description` names the action
/// for the error context.
fn run_tool(command: &mut Command, description: &str) -> anyhow::Result<()> {
    let output = command.output().with_context(|| description.to_string())?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "{description} failed ({}): {}",
            output.status,
            stderr.trim()
        );
    }
    Ok(())
}

pub fn system_commands() -> Vec<Box<dyn SystemCommand>> {
    vec![
        Box::new(power::LockScreen),
        Box::new(power::Sleep),
        Box::new(power::Restart),
        Box::new(power::Shutdown),
        Box::new(power::LogOut),
        Box::new(appearance::ToggleAppearance),
        Box::new(utilities::EmptyTrash),
        Box::new(utilities::StartScreenSaver),
        Box::new(utilities::EjectDisc),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_tool_succeeds_on_zero_exit() {
        run_tool(&mut Command::new("true"), "run true").expect("true exits 0");
    }

    #[test]
    fn run_tool_fails_on_non_zero_exit_with_stderr() {
        let err = run_tool(
            Command::new("sh").args(["-c", "echo 'no drive found' >&2; exit 3"]),
            "eject disc via drutil",
        )
        .expect_err("exit 3 is a failure");
        let message = format!("{err:#}");
        assert!(message.contains("eject disc via drutil"), "{message}");
        assert!(message.contains("exit status: 3"), "{message}");
        assert!(message.contains("no drive found"), "{message}");
    }

    #[test]
    fn run_tool_fails_when_the_tool_is_missing() {
        let err = run_tool(
            &mut Command::new("torchsnap-test-no-such-tool"),
            "run a missing tool",
        )
        .expect_err("spawn fails");
        assert!(format!("{err:#}").contains("run a missing tool"));
    }
}
