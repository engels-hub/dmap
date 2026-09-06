//! Choice of the display that shows the players' view.

// Rust guideline compliant 2026-02-21

use crate::project::TvPlacement;

/// Picks the display for the TV: the first one that is not the DM's.
///
/// Returns the index into `displays`, or `None` when every display is the
/// DM's. Then the TV should open as a normal window, so it does not cover
/// the DM window.
pub fn pick_tv_display<T: PartialEq>(displays: &[T], dm_display: Option<&T>) -> Option<usize> {
    displays
        .iter()
        .position(|display| Some(display) != dm_display)
}

/// Turns a saved placement into a display index.
///
/// `names` are the connected displays in order. `dm_display` is the index of
/// the DM's display. A saved name that is no longer connected falls back to
/// the automatic choice. `None` means a normal window.
pub fn resolve_tv_display<S: AsRef<str>>(
    placement: &TvPlacement,
    names: &[Option<S>],
    dm_display: Option<usize>,
) -> Option<usize> {
    let auto = || {
        let indices: Vec<usize> = (0..names.len()).collect();
        pick_tv_display(&indices, dm_display.as_ref())
    };
    match placement {
        TvPlacement::Auto => auto(),
        TvPlacement::Window => None,
        TvPlacement::Display(wanted) => names
            .iter()
            .position(|name| name.as_ref().is_some_and(|name| name.as_ref() == wanted))
            .or_else(auto),
    }
}

/// A display's `(position, size)` in screen pixels.
pub type DisplayRect = ((i32, i32), (u32, u32));

/// Finds the display whose rectangle holds `point`.
///
/// `rects` are in display order.
pub fn display_at(point: (i32, i32), rects: &[DisplayRect]) -> Option<usize> {
    let (px, py) = (i64::from(point.0), i64::from(point.1));
    rects.iter().position(|&((x, y), (width, height))| {
        let (x, y) = (i64::from(x), i64::from(y));
        px >= x && py >= y && px - x < i64::from(width) && py - y < i64::from(height)
    })
}

/// Where the DM window must move so it does not sit on the TV display.
///
/// Returns the index of a free display when the TV and the DM window share a
/// display and another one exists. Otherwise `None`: nothing moves.
pub fn dm_move_target(
    tv_display: Option<usize>,
    dm_display: Option<usize>,
    display_count: usize,
) -> Option<usize> {
    let tv = tv_display?;
    if dm_display != Some(tv) {
        return None;
    }
    (0..display_count).find(|&i| i != tv)
}

/// Turns the picker's choice into what the project file stores.
///
/// A display without a name cannot be found again by name, so it is saved
/// as the automatic choice.
pub fn placement_for<S: AsRef<str>>(index: Option<usize>, names: &[Option<S>]) -> TvPlacement {
    match index {
        None => TvPlacement::Window,
        Some(i) => names[i].as_ref().map_or(TvPlacement::Auto, |name| {
            TvPlacement::Display(name.as_ref().to_owned())
        }),
    }
}

/// Human-readable label for a display: its name and its resolution.
pub fn display_label(name: Option<&str>, width: u32, height: u32) -> String {
    format!("{} · {width} × {height}", name.unwrap_or("Display"))
}

#[cfg(test)]
mod tests {
    use super::{
        display_at, display_label, dm_move_target, pick_tv_display, placement_for,
        resolve_tv_display,
    };

    #[test]
    fn finds_the_display_that_holds_a_point() {
        let rects = [((0, 0), (1920, 1200)), ((1920, 0), (2560, 1440))];
        assert_eq!(display_at((5, 29), &rects), Some(0));
        assert_eq!(display_at((1920, 0), &rects), Some(1));
        assert_eq!(display_at((1925, 1439), &rects), Some(1));
    }

    #[test]
    fn finds_no_display_for_a_point_outside_all() {
        let rects = [((0, 0), (1920, 1200)), ((1920, 0), (2560, 1440))];
        assert_eq!(display_at((-1, 0), &rects), None);
        assert_eq!(display_at((4480, 0), &rects), None);
        assert_eq!(display_at((0, 1200), &rects), None);
    }

    /// A landscape primary display next to a portrait one, offset so it
    /// covers negative coordinates. Reproduces a real two-monitor layout
    /// where a maximized window's outer top-left corner (an invisible
    /// resize border puts it a few pixels off its own monitor on Windows)
    /// landed inside the portrait display's rectangle, misclassifying the
    /// window and sending it into a move-resize loop. The window's center
    /// stays correctly on its own display, which is why `display_of` in
    /// `main.rs` classifies by the window's center rather than its corner.
    #[test]
    fn a_maximized_windows_corner_can_land_on_the_next_display_but_its_center_does_not() {
        let rects = [((0, 0), (1920, 1080)), ((-1080, -495), (1080, 1920))];
        let maximized_outer_corner = (-8, -8);
        assert_eq!(display_at(maximized_outer_corner, &rects), Some(1));
        let maximized_center = (1920 / 2, 1080 / 2);
        assert_eq!(display_at(maximized_center, &rects), Some(0));
    }
    use crate::project::TvPlacement;

    #[test]
    fn moves_the_dm_window_off_the_display_that_became_the_tv() {
        assert_eq!(dm_move_target(Some(0), Some(0), 2), Some(1));
        assert_eq!(dm_move_target(Some(1), Some(1), 3), Some(0));
    }

    #[test]
    fn leaves_the_dm_window_alone_when_the_tv_is_elsewhere() {
        assert_eq!(dm_move_target(Some(1), Some(0), 2), None);
        assert_eq!(dm_move_target(None, Some(0), 2), None);
        assert_eq!(dm_move_target(Some(0), None, 2), None);
    }

    #[test]
    fn leaves_the_dm_window_alone_when_there_is_no_other_display() {
        assert_eq!(dm_move_target(Some(0), Some(0), 1), None);
    }

    #[test]
    fn saves_a_picked_display_by_name() {
        let names = [Some("a"), None];
        assert_eq!(
            placement_for(Some(0), &names),
            TvPlacement::Display("a".to_owned())
        );
        assert_eq!(placement_for(None, &names), TvPlacement::Window);
    }

    #[test]
    fn saves_an_unnamed_display_as_auto() {
        let names = [Some("a"), None];
        assert_eq!(placement_for(Some(1), &names), TvPlacement::Auto);
    }

    #[test]
    fn resolves_auto_to_a_free_display() {
        let names = [Some("a"), Some("b")];
        assert_eq!(
            resolve_tv_display(&TvPlacement::Auto, &names, Some(0)),
            Some(1)
        );
    }

    #[test]
    fn resolves_window_to_none() {
        let names = [Some("a"), Some("b")];
        assert_eq!(
            resolve_tv_display(&TvPlacement::Window, &names, Some(0)),
            None
        );
    }

    #[test]
    fn resolves_a_saved_name_to_that_display() {
        let names = [Some("a"), None, Some("c")];
        let saved = TvPlacement::Display("c".to_owned());
        assert_eq!(resolve_tv_display(&saved, &names, Some(0)), Some(2));
    }

    #[test]
    fn falls_back_to_a_free_display_when_the_saved_name_is_gone() {
        let names = [Some("a"), Some("b")];
        let saved = TvPlacement::Display("gone".to_owned());
        assert_eq!(resolve_tv_display(&saved, &names, Some(0)), Some(1));
    }

    #[test]
    fn labels_a_display_with_its_name_and_size() {
        assert_eq!(
            display_label(Some("HDMI-1"), 3840, 2160),
            "HDMI-1 · 3840 × 2160"
        );
    }

    #[test]
    fn labels_an_unnamed_display_generically() {
        assert_eq!(display_label(None, 1920, 1080), "Display · 1920 × 1080");
    }

    #[test]
    fn picks_first_display_that_is_not_the_dm_display() {
        assert_eq!(pick_tv_display(&["a", "b", "c"], Some(&"a")), Some(1));
        assert_eq!(pick_tv_display(&["a", "b", "c"], Some(&"b")), Some(0));
    }

    #[test]
    fn picks_first_display_when_dm_display_is_unknown() {
        assert_eq!(pick_tv_display(&["a", "b"], None), Some(0));
    }

    #[test]
    fn picks_none_when_only_the_dm_display_exists() {
        assert_eq!(pick_tv_display(&["a"], Some(&"a")), None);
        assert_eq!(pick_tv_display::<&str>(&[], None), None);
    }
}
