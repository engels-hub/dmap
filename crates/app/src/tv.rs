//! Choice of the display that shows the players' view.

// Rust guideline compliant 2026-02-21

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

/// Human-readable label for a display: its name and its resolution.
pub fn display_label(name: Option<&str>, width: u32, height: u32) -> String {
    format!("{} · {width} × {height}", name.unwrap_or("Display"))
}

#[cfg(test)]
mod tests {
    use super::{display_label, pick_tv_display};

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
