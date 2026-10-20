// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Which monitor the cursor is on, and where a window goes on it.
//!
//! The functions here take the cursor position and monitor geometry as
//! the windowing backend reports them, so they run in tests without
//! real displays. The callers in `lib.rs` read those values from Tauri.
//!
//! On macOS those values are not in one space. tao (0.35) derives all of
//! them from points but scales each by a different factor: a monitor's
//! position and size by that monitor's own scale factor, the cursor
//! position by the primary monitor's. With a Retina display next to a
//! non-Retina one, the raw cursor falls outside the monitor it is on or
//! inside another one. `CoordinateSpace::MacosPoints` turns everything
//! back into points before comparing.

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

/// How the reported cursor position and window sizes relate to the
/// reported monitor rects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoordinateSpace {
    /// Cursor and monitor rects compare as reported. Used on every
    /// platform but macOS.
    Shared,
    /// tao on macOS: a monitor rect is its rect in points times the
    /// monitor's own scale factor, the cursor its position in points
    /// times the primary monitor's scale factor, a window's outer size
    /// its size in points times the window's own scale factor.
    MacosPoints,
}

/// The coordinate space of the platform this build is for. Written with
/// `cfg!` rather than `#[cfg]` so both variants exist on every target.
pub(crate) const PLATFORM_COORDINATE_SPACE: CoordinateSpace = if cfg!(target_os = "macos") {
    CoordinateSpace::MacosPoints
} else {
    CoordinateSpace::Shared
};

/// Index of the first monitor whose rect contains the cursor.
///
/// Rects are half-open, so a cursor on the edge two monitors share
/// belongs to the right-hand or lower one. `Shared` does not use
/// `primary_scale_factor`; `MacosPoints` needs it to bring the cursor
/// back to points and finds no monitor without it.
pub(crate) fn monitor_index_under_cursor(
    cursor: PhysicalPosition<f64>,
    monitors: &[ReportedMonitor],
    primary_scale_factor: Option<f64>,
    space: CoordinateSpace,
) -> Option<usize> {
    let (cursor_x, cursor_y) = match space {
        CoordinateSpace::Shared => (cursor.x, cursor.y),
        CoordinateSpace::MacosPoints => {
            let scale = primary_scale_factor?;
            (cursor.x / scale, cursor.y / scale)
        }
    };

    monitors.iter().position(|monitor| {
        // Dividing by 1.0 leaves the reported values exactly as they are.
        let scale = match space {
            CoordinateSpace::Shared => 1.0,
            CoordinateSpace::MacosPoints => monitor.scale_factor,
        };
        let x = monitor.position.x as f64 / scale;
        let y = monitor.position.y as f64 / scale;
        let w = monitor.size.width as f64 / scale;
        let h = monitor.size.height as f64 / scale;
        cursor_x >= x && cursor_x < x + w && cursor_y >= y && cursor_y < y + h
    })
}

/// Top-left origin in logical points that centers a window of
/// `window_size` (its reported outer size) on `monitor`.
///
/// `Shared` divides the window size by the monitor's scale factor and
/// does not use `window_scale_factor`. `MacosPoints` divides it by the
/// window's own scale factor, the one tao scaled it with, and gives no
/// origin without it.
pub(crate) fn centered_window_origin(
    monitor: &ReportedMonitor,
    window_size: PhysicalSize<u32>,
    window_scale_factor: Option<f64>,
    space: CoordinateSpace,
) -> Option<LogicalPosition<f64>> {
    // Divided by hand rather than with `dpi`'s `to_logical`, which
    // asserts on the scale factor.
    let scale = monitor.scale_factor;
    let window_scale = match space {
        CoordinateSpace::Shared => scale,
        CoordinateSpace::MacosPoints => window_scale_factor?,
    };
    let x = monitor.position.x as f64 / scale
        + (monitor.size.width as f64 / scale - window_size.width as f64 / window_scale) / 2.0;
    let y = monitor.position.y as f64 / scale
        + (monitor.size.height as f64 / scale - window_size.height as f64 / window_scale) / 2.0;
    Some(LogicalPosition::new(x, y))
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
    fn shared_choice_finds_the_only_monitor() {
        let monitors = [monitor(0, 0, 1920, 1080, 1.0)];
        assert_eq!(
            monitor_index_under_cursor(
                cursor(960.0, 540.0),
                &monitors,
                None,
                CoordinateSpace::Shared
            ),
            Some(0)
        );
    }

    #[test]
    fn shared_choice_finds_the_second_of_two_side_by_side() {
        let monitors = [
            monitor(0, 0, 1920, 1080, 1.0),
            monitor(1920, 0, 2560, 1440, 1.0),
        ];
        assert_eq!(
            monitor_index_under_cursor(
                cursor(3000.0, 1200.0),
                &monitors,
                None,
                CoordinateSpace::Shared
            ),
            Some(1)
        );
    }

    #[test]
    fn shared_choice_finds_a_monitor_left_of_the_primary() {
        let monitors = [
            monitor(0, 0, 1920, 1080, 1.0),
            monitor(-2560, 0, 2560, 1440, 1.0),
        ];
        assert_eq!(
            monitor_index_under_cursor(
                cursor(-100.0, 700.0),
                &monitors,
                None,
                CoordinateSpace::Shared
            ),
            Some(1)
        );
    }

    #[test]
    fn shared_choice_finds_a_monitor_above_the_primary() {
        let monitors = [
            monitor(0, 0, 1920, 1080, 1.0),
            monitor(-320, -1440, 2560, 1440, 1.0),
        ];
        assert_eq!(
            monitor_index_under_cursor(
                cursor(100.0, -10.0),
                &monitors,
                None,
                CoordinateSpace::Shared
            ),
            Some(1)
        );
    }

    #[test]
    fn shared_choice_finds_nothing_outside_every_monitor() {
        let monitors = [
            monitor(0, 0, 1920, 1080, 1.0),
            monitor(1920, 0, 2560, 1440, 1.0),
        ];
        assert_eq!(
            monitor_index_under_cursor(
                cursor(1000.0, 1200.0),
                &monitors,
                None,
                CoordinateSpace::Shared
            ),
            None
        );
    }

    #[test]
    fn shared_choice_puts_a_shared_edge_on_the_right_hand_monitor() {
        let monitors = [
            monitor(0, 0, 1920, 1080, 1.0),
            monitor(1920, 0, 2560, 1440, 1.0),
        ];
        assert_eq!(
            monitor_index_under_cursor(
                cursor(1920.0, 0.0),
                &monitors,
                None,
                CoordinateSpace::Shared
            ),
            Some(1)
        );
    }

    #[test]
    fn shared_choice_finds_nothing_at_the_far_edge() {
        let monitors = [
            monitor(0, 0, 1920, 1080, 1.0),
            monitor(1920, 0, 2560, 1440, 1.0),
        ];
        assert_eq!(
            monitor_index_under_cursor(
                cursor(4480.0, 0.0),
                &monitors,
                None,
                CoordinateSpace::Shared
            ),
            None
        );
        assert_eq!(
            monitor_index_under_cursor(
                cursor(0.0, 1080.0),
                &monitors,
                None,
                CoordinateSpace::Shared
            ),
            None
        );
    }

    #[test]
    fn shared_choice_takes_the_first_of_overlapping_monitors() {
        let monitors = [
            monitor(0, 0, 1920, 1080, 1.0),
            monitor(0, 0, 1920, 1080, 2.0),
        ];
        assert_eq!(
            monitor_index_under_cursor(
                cursor(10.0, 10.0),
                &monitors,
                None,
                CoordinateSpace::Shared
            ),
            Some(0)
        );
    }

    #[test]
    fn shared_choice_finds_nothing_without_monitors() {
        assert_eq!(
            monitor_index_under_cursor(cursor(0.0, 0.0), &[], None, CoordinateSpace::Shared),
            None
        );
    }

    #[test]
    fn shared_choice_matches_without_a_primary_scale() {
        let monitors = [
            monitor(0, 0, 1920, 1080, 1.0),
            monitor(1920, 0, 1920, 1080, 1.0),
        ];
        assert_eq!(
            monitor_index_under_cursor(
                cursor(2000.0, 100.0),
                &monitors,
                None,
                CoordinateSpace::Shared
            ),
            Some(1)
        );
    }

    #[test]
    fn shared_choice_ignores_the_primary_scale() {
        // The values tao reports on macOS for the cursor at (3900, 10)
        // points on the external display of the arrangement in
        // `retina_primary_with_external_on_the_right`. `Shared` compares
        // them raw, so no monitor matches.
        let monitors = retina_primary_with_external_on_the_right();
        assert_eq!(
            monitor_index_under_cursor(
                cursor(7800.0, 20.0),
                &monitors,
                Some(2.0),
                CoordinateSpace::Shared
            ),
            None
        );
    }

    #[test]
    fn platform_space_matches_the_target_os() {
        if cfg!(target_os = "macos") {
            assert_eq!(PLATFORM_COORDINATE_SPACE, CoordinateSpace::MacosPoints);
        } else {
            assert_eq!(PLATFORM_COORDINATE_SPACE, CoordinateSpace::Shared);
        }
    }

    // =========================================================
    // Monitor choice in macOS points
    //
    // The helpers build values the way tao reports them on macOS, so
    // each arrangement is written in points, as macOS describes it.
    // =========================================================

    /// A monitor at (`x`, `y`) points, `width` by `height` points, as
    /// tao reports it: every value times the monitor's own scale factor.
    fn macos_monitor(
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        scale_factor: f64,
    ) -> ReportedMonitor {
        ReportedMonitor {
            position: PhysicalPosition::new(
                (x as f64 * scale_factor).round() as i32,
                (y as f64 * scale_factor).round() as i32,
            ),
            size: PhysicalSize::new(
                (width as f64 * scale_factor).round() as u32,
                (height as f64 * scale_factor).round() as u32,
            ),
            scale_factor,
        }
    }

    /// The cursor at (`x`, `y`) points as tao reports it: times the
    /// primary monitor's scale factor.
    fn macos_cursor(x: f64, y: f64, primary_scale_factor: f64) -> PhysicalPosition<f64> {
        PhysicalPosition::new(x * primary_scale_factor, y * primary_scale_factor)
    }

    /// A 2x MacBook display as primary, a 1x external display right of it.
    fn retina_primary_with_external_on_the_right() -> [ReportedMonitor; 2] {
        [
            macos_monitor(0, 0, 1512, 982, 2.0),
            macos_monitor(1512, 0, 2560, 1440, 1.0),
        ]
    }

    fn macos_choice(
        x: f64,
        y: f64,
        monitors: &[ReportedMonitor],
        primary_scale_factor: f64,
    ) -> Option<usize> {
        monitor_index_under_cursor(
            macos_cursor(x, y, primary_scale_factor),
            monitors,
            Some(primary_scale_factor),
            CoordinateSpace::MacosPoints,
        )
    }

    #[test]
    fn macos_choice_finds_the_external_1x_monitor_at_its_menu_bar() {
        let monitors = retina_primary_with_external_on_the_right();
        assert_eq!(macos_choice(3900.0, 10.0, &monitors, 2.0), Some(1));
    }

    #[test]
    fn macos_choice_finds_the_external_1x_monitor_below_half_its_height() {
        let monitors = retina_primary_with_external_on_the_right();
        assert_eq!(macos_choice(2000.0, 1000.0, &monitors, 2.0), Some(1));
    }

    #[test]
    fn macos_choice_finds_the_2x_primary_when_listed_after_the_external() {
        let [primary, external] = retina_primary_with_external_on_the_right();
        let monitors = [external, primary];
        assert_eq!(macos_choice(1000.0, 100.0, &monitors, 2.0), Some(1));
    }

    #[test]
    fn macos_choice_finds_a_2x_monitor_right_of_a_1x_primary() {
        let monitors = [
            macos_monitor(0, 0, 1920, 1080, 1.0),
            macos_monitor(1920, 0, 1512, 982, 2.0),
        ];
        assert_eq!(macos_choice(2500.0, 300.0, &monitors, 1.0), Some(1));
    }

    #[test]
    fn macos_choice_finds_a_1x_monitor_right_of_a_2x_secondary() {
        let monitors = [
            macos_monitor(0, 0, 1920, 1080, 1.0),
            macos_monitor(1920, 0, 1512, 982, 2.0),
            macos_monitor(3432, 0, 1920, 1080, 1.0),
        ];
        assert_eq!(macos_choice(4000.0, 100.0, &monitors, 1.0), Some(2));
    }

    #[test]
    fn macos_choice_finds_a_1x_monitor_left_of_a_2x_primary() {
        let monitors = [
            macos_monitor(0, 0, 1512, 982, 2.0),
            macos_monitor(-2560, 0, 2560, 1440, 1.0),
        ];
        assert_eq!(macos_choice(-100.0, 900.0, &monitors, 2.0), Some(1));
    }

    #[test]
    fn macos_choice_finds_a_1x_monitor_above_a_2x_primary() {
        let monitors = [
            macos_monitor(0, 0, 1512, 982, 2.0),
            macos_monitor(-500, -1440, 2560, 1440, 1.0),
        ];
        assert_eq!(macos_choice(1500.0, -1000.0, &monitors, 2.0), Some(1));
    }

    #[test]
    fn macos_choice_finds_a_2x_monitor_below_a_1x_primary() {
        let monitors = [
            macos_monitor(0, 0, 1920, 1080, 1.0),
            macos_monitor(0, 1080, 1512, 982, 2.0),
        ];
        assert_eq!(macos_choice(100.0, 1500.0, &monitors, 1.0), Some(1));
    }

    #[test]
    fn macos_choice_finds_the_2x_primary_when_listed_first() {
        let monitors = retina_primary_with_external_on_the_right();
        assert_eq!(macos_choice(700.0, 500.0, &monitors, 2.0), Some(0));
    }

    #[test]
    fn macos_choice_with_equal_scale_factors_matches_the_shared_comparison() {
        let monitors = [
            macos_monitor(0, 0, 1512, 982, 2.0),
            macos_monitor(1512, 0, 1512, 982, 2.0),
        ];
        assert_eq!(macos_choice(2000.0, 100.0, &monitors, 2.0), Some(1));
    }

    #[test]
    fn macos_choice_finds_nothing_outside_every_monitor() {
        let monitors = retina_primary_with_external_on_the_right();
        assert_eq!(macos_choice(5000.0, 10.0, &monitors, 2.0), None);
    }

    #[test]
    fn macos_choice_puts_the_shared_edge_on_the_external() {
        let monitors = retina_primary_with_external_on_the_right();
        assert_eq!(macos_choice(1512.0, 0.0, &monitors, 2.0), Some(1));
    }

    #[test]
    fn macos_choice_finds_nothing_without_a_primary_scale() {
        let monitors = retina_primary_with_external_on_the_right();
        assert_eq!(
            monitor_index_under_cursor(
                macos_cursor(700.0, 500.0, 2.0),
                &monitors,
                None,
                CoordinateSpace::MacosPoints
            ),
            None
        );
    }

    // =========================================================
    // Window centering
    // =========================================================

    #[test]
    fn shared_centered_origin_on_a_1x_monitor_for_a_1x_window() {
        let target = monitor(1512, 0, 2560, 1440, 1.0);
        assert_eq!(
            centered_window_origin(
                &target,
                PhysicalSize::new(720, 520),
                Some(1.0),
                CoordinateSpace::Shared
            ),
            Some(LogicalPosition::new(2432.0, 460.0))
        );
    }

    #[test]
    fn shared_centered_origin_on_a_2x_monitor_for_a_2x_window() {
        let target = monitor(0, 0, 3024, 1964, 2.0);
        assert_eq!(
            centered_window_origin(
                &target,
                PhysicalSize::new(1440, 1040),
                Some(2.0),
                CoordinateSpace::Shared
            ),
            Some(LogicalPosition::new(396.0, 231.0))
        );
    }

    #[test]
    fn shared_centered_origin_on_a_monitor_left_of_the_primary() {
        let target = monitor(-2560, -200, 2560, 1440, 1.0);
        assert_eq!(
            centered_window_origin(
                &target,
                PhysicalSize::new(720, 520),
                Some(1.0),
                CoordinateSpace::Shared
            ),
            Some(LogicalPosition::new(-1640.0, 260.0))
        );
    }

    #[test]
    fn shared_centered_origin_ignores_the_window_scale() {
        let target = monitor(1512, 0, 2560, 1440, 1.0);
        assert_eq!(
            centered_window_origin(
                &target,
                PhysicalSize::new(1440, 1040),
                Some(2.0),
                CoordinateSpace::Shared
            ),
            Some(LogicalPosition::new(2072.0, 200.0))
        );
    }

    #[test]
    fn shared_centered_origin_ignores_a_missing_window_scale() {
        let target = monitor(1512, 0, 2560, 1440, 1.0);
        assert_eq!(
            centered_window_origin(
                &target,
                PhysicalSize::new(1440, 1040),
                None,
                CoordinateSpace::Shared
            ),
            Some(LogicalPosition::new(2072.0, 200.0))
        );
    }

    // =========================================================
    // Window centering in macOS points
    // =========================================================

    #[test]
    fn macos_centered_origin_on_a_1x_monitor_for_a_window_on_a_2x_screen() {
        let target = macos_monitor(1512, 0, 2560, 1440, 1.0);
        // 720 by 520 points on a 2x screen.
        let window_size = PhysicalSize::new(1440, 1040);
        assert_eq!(
            centered_window_origin(
                &target,
                window_size,
                Some(2.0),
                CoordinateSpace::MacosPoints
            ),
            Some(LogicalPosition::new(2432.0, 460.0))
        );
    }

    #[test]
    fn macos_centered_origin_on_a_2x_monitor_for_a_window_on_a_1x_screen() {
        let target = macos_monitor(0, 0, 1512, 982, 2.0);
        // 720 by 520 points on a 1x screen.
        let window_size = PhysicalSize::new(720, 520);
        assert_eq!(
            centered_window_origin(
                &target,
                window_size,
                Some(1.0),
                CoordinateSpace::MacosPoints
            ),
            Some(LogicalPosition::new(396.0, 231.0))
        );
    }

    #[test]
    fn macos_centered_origin_with_equal_scale_factors() {
        let target = macos_monitor(0, 0, 1512, 982, 2.0);
        assert_eq!(
            centered_window_origin(
                &target,
                PhysicalSize::new(1440, 1040),
                Some(2.0),
                CoordinateSpace::MacosPoints
            ),
            Some(LogicalPosition::new(396.0, 231.0))
        );
    }

    #[test]
    fn macos_centered_origin_is_none_without_a_window_scale() {
        let target = macos_monitor(1512, 0, 2560, 1440, 1.0);
        assert_eq!(
            centered_window_origin(
                &target,
                PhysicalSize::new(1440, 1040),
                None,
                CoordinateSpace::MacosPoints
            ),
            None
        );
    }
}
