// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Which monitor the cursor is on, and where a window goes on it.
//!
//! The functions here take the cursor position and monitor geometry as
//! the windowing backend reports them, so they run in tests without
//! real displays. The callers in `lib.rs` read those values from Tauri.

use tauri::{LogicalPosition, PhysicalPosition, PhysicalSize};

/// A monitor's geometry as the windowing backend reports it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ReportedMonitor {
    pub(crate) position: PhysicalPosition<i32>,
    pub(crate) size: PhysicalSize<u32>,
    pub(crate) scale_factor: f64,
}

impl ReportedMonitor {
    pub(crate) fn of(monitor: &tauri::Monitor) -> Self {
        Self {
            position: *monitor.position(),
            size: *monitor.size(),
            scale_factor: monitor.scale_factor(),
        }
    }
}

/// Index of the first monitor whose rect contains the cursor.
///
/// Rects are half-open, so a cursor on the edge two monitors share
/// belongs to the right-hand or lower one.
pub(crate) fn monitor_index_under_cursor(
    cursor: PhysicalPosition<f64>,
    monitors: &[ReportedMonitor],
) -> Option<usize> {
    monitors.iter().position(|monitor| {
        let x = monitor.position.x as f64;
        let y = monitor.position.y as f64;
        let w = monitor.size.width as f64;
        let h = monitor.size.height as f64;
        cursor.x >= x && cursor.x < x + w && cursor.y >= y && cursor.y < y + h
    })
}

/// Top-left origin in logical points that centers a window of
/// `window_size` on `monitor`.
pub(crate) fn centered_window_origin(
    monitor: &ReportedMonitor,
    window_size: PhysicalSize<u32>,
) -> LogicalPosition<f64> {
    // Divided by hand rather than with `dpi`'s `to_logical`, which
    // asserts on the scale factor.
    let scale = monitor.scale_factor;
    let x = monitor.position.x as f64 / scale
        + (monitor.size.width as f64 / scale - window_size.width as f64 / scale) / 2.0;
    let y = monitor.position.y as f64 / scale
        + (monitor.size.height as f64 / scale - window_size.height as f64 / scale) / 2.0;
    LogicalPosition::new(x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(x: i32, y: i32, width: u32, height: u32, scale_factor: f64) -> ReportedMonitor {
        ReportedMonitor {
            position: PhysicalPosition::new(x, y),
            size: PhysicalSize::new(width, height),
            scale_factor,
        }
    }

    fn cursor(x: f64, y: f64) -> PhysicalPosition<f64> {
        PhysicalPosition::new(x, y)
    }

    // =========================================================
    // Monitor choice
    // =========================================================

    #[test]
    fn monitor_choice_finds_the_only_monitor() {
        let monitors = [monitor(0, 0, 1920, 1080, 1.0)];
        assert_eq!(
            monitor_index_under_cursor(cursor(960.0, 540.0), &monitors),
            Some(0)
        );
    }

    #[test]
    fn monitor_choice_finds_the_second_of_two_side_by_side() {
        let monitors = [
            monitor(0, 0, 1920, 1080, 1.0),
            monitor(1920, 0, 2560, 1440, 1.0),
        ];
        assert_eq!(
            monitor_index_under_cursor(cursor(3000.0, 1200.0), &monitors),
            Some(1)
        );
    }

    #[test]
    fn monitor_choice_finds_a_monitor_left_of_the_primary() {
        let monitors = [
            monitor(0, 0, 1920, 1080, 1.0),
            monitor(-2560, 0, 2560, 1440, 1.0),
        ];
        assert_eq!(
            monitor_index_under_cursor(cursor(-100.0, 700.0), &monitors),
            Some(1)
        );
    }

    #[test]
    fn monitor_choice_finds_a_monitor_above_the_primary() {
        let monitors = [
            monitor(0, 0, 1920, 1080, 1.0),
            monitor(-320, -1440, 2560, 1440, 1.0),
        ];
        assert_eq!(
            monitor_index_under_cursor(cursor(100.0, -10.0), &monitors),
            Some(1)
        );
    }

    #[test]
    fn monitor_choice_finds_nothing_outside_every_monitor() {
        let monitors = [
            monitor(0, 0, 1920, 1080, 1.0),
            monitor(1920, 0, 2560, 1440, 1.0),
        ];
        assert_eq!(
            monitor_index_under_cursor(cursor(1000.0, 1200.0), &monitors),
            None
        );
    }

    #[test]
    fn monitor_choice_puts_a_shared_edge_on_the_right_hand_monitor() {
        let monitors = [
            monitor(0, 0, 1920, 1080, 1.0),
            monitor(1920, 0, 2560, 1440, 1.0),
        ];
        assert_eq!(
            monitor_index_under_cursor(cursor(1920.0, 0.0), &monitors),
            Some(1)
        );
    }

    #[test]
    fn monitor_choice_finds_nothing_at_the_far_edge() {
        let monitors = [
            monitor(0, 0, 1920, 1080, 1.0),
            monitor(1920, 0, 2560, 1440, 1.0),
        ];
        assert_eq!(
            monitor_index_under_cursor(cursor(4480.0, 0.0), &monitors),
            None
        );
        assert_eq!(
            monitor_index_under_cursor(cursor(0.0, 1080.0), &monitors),
            None
        );
    }

    #[test]
    fn monitor_choice_takes_the_first_of_overlapping_monitors() {
        let monitors = [
            monitor(0, 0, 1920, 1080, 1.0),
            monitor(0, 0, 1920, 1080, 2.0),
        ];
        assert_eq!(
            monitor_index_under_cursor(cursor(10.0, 10.0), &monitors),
            Some(0)
        );
    }

    #[test]
    fn monitor_choice_finds_nothing_without_monitors() {
        assert_eq!(monitor_index_under_cursor(cursor(0.0, 0.0), &[]), None);
    }

    // =========================================================
    // Window centering
    // =========================================================

    #[test]
    fn centered_origin_on_a_1x_monitor_for_a_1x_window() {
        let target = monitor(1512, 0, 2560, 1440, 1.0);
        assert_eq!(
            centered_window_origin(&target, PhysicalSize::new(720, 520)),
            LogicalPosition::new(2432.0, 460.0)
        );
    }

    #[test]
    fn centered_origin_on_a_2x_monitor_for_a_2x_window() {
        let target = monitor(0, 0, 3024, 1964, 2.0);
        assert_eq!(
            centered_window_origin(&target, PhysicalSize::new(1440, 1040)),
            LogicalPosition::new(396.0, 231.0)
        );
    }

    #[test]
    fn centered_origin_on_a_monitor_left_of_the_primary() {
        let target = monitor(-2560, -200, 2560, 1440, 1.0);
        assert_eq!(
            centered_window_origin(&target, PhysicalSize::new(720, 520)),
            LogicalPosition::new(-1640.0, 260.0)
        );
    }
}
