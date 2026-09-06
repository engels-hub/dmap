//! Choice of the display that shows the players' view.

// Rust guideline compliant 2026-02-21

/// Picks the display for the TV: the first one that is not the DM's.
///
/// Returns the index into `displays`, or `None` when every display is the
/// DM's. Then the TV should open as a normal window, so it does not cover
/// the DM window.
#[expect(dead_code, reason = "used once the TV window opens")]
pub fn pick_tv_display<T: PartialEq>(displays: &[T], dm_display: Option<&T>) -> Option<usize> {
    displays
        .iter()
        .position(|display| Some(display) != dm_display)
}

#[cfg(test)]
mod tests {
    use super::pick_tv_display;

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
