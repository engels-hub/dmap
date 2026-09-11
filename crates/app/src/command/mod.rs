//! Every change to the scene, and the stack that walks back over them.
//!
//! A tool never writes the scene. It builds a [`Command`] and hands it to
//! the [`History`], which writes it. The history keeps what it wrote, so
//! `Ctrl+Z` puts the scene back and `Ctrl+Shift+Z` writes it again.
//!
//! A change under the DM's hand, such as a drag, comes in through
//! [`History::hold`] once a frame. The history drops the one it held and
//! keeps the new one, so a drag of a hundred frames is one step. The tool
//! calls [`History::settle`] when the drag ends. A change that is over in
//! one frame, such as a key press or a click, goes through
//! [`History::run`].
//!
//! A change holds the values it writes and the values that stood there
//! before, so every revert is exact. PLAN.md 5.1.

// Rust guideline compliant 2026-02-21

mod change;
mod history;

pub use change::{
    Deed, Grow, Restructure, SetAssets, SetName, SetShown, SetStrokes, SetTvBox, Turn, reshape,
};
pub use history::History;

use std::time::{SystemTime, UNIX_EPOCH};

use crate::scene::{Asset, Scene, Shown};
use crate::text;

/// What one step says on the history list. DESIGN.md 9.8.
///
/// A change fills the first three. The history fills `ago`, because a
/// change does not know what the time is.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Note {
    /// What the DM did, such as `Move` or `New group`.
    pub what: String,
    /// The file or the group it happened to.
    pub subject: String,
    /// The numbers the change wrote, such as the two spots of a move.
    pub detail: String,
    /// How long ago the DM made it, such as `4 min ago`.
    pub ago: String,
}

impl Note {
    /// A note with no time on it yet.
    fn new(what: &str, subject: String, detail: String) -> Self {
        Self {
            what: what.to_owned(),
            subject,
            detail,
            ago: String::new(),
        }
    }
}

/// One change to the scene, which the history writes and takes back.
pub trait Command: std::fmt::Debug {
    /// Writes the change into `scene`.
    fn apply(&self, scene: &mut Scene);

    /// Puts `scene` back the way it stood before [`Command::apply`].
    fn revert(&self, scene: &mut Scene);

    /// What this change says on the history list. DESIGN.md 9.8.
    fn note(&self) -> Note;
}

/// The seconds since the epoch, for the time on a step.
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// How long ago a step was made, in words. DESIGN.md 9.8.
///
/// A step from a file that an older version of the program wrote carries
/// no time, and reads as `earlier`.
fn ago(at: u64, now: u64) -> String {
    if at == 0 {
        return text::history_ago_earlier().to_owned();
    }
    // A clock that went back leaves a step in the future. It happened, so
    // it reads as the newest thing that happened.
    let gap = now.saturating_sub(at);
    // Under three quarters of a minute reads as no time at all, as it
    // does in a chat window.
    if gap < 45 {
        return text::history_ago_now().to_owned();
    }
    let minutes = (gap + 30) / 60;
    if minutes < 90 {
        return text::history_ago_minutes(minutes.max(1));
    }
    let hours = (gap + 1800) / 3600;
    if hours < 24 {
        return text::history_ago_hours(hours);
    }
    // A day and a half still reads as a day. The DM wants to know which
    // session a step belongs to, not the hour of it.
    if gap < 2 * 86400 {
        return text::history_ago_day().to_owned();
    }
    text::history_ago_days(gap / 86400)
}

/// The name of a spot on the canvas, in inches.
fn spot(at: (f64, f64)) -> String {
    text::history_spot(format_args!("{:.1}", at.0), format_args!("{:.1}", at.1))
}

/// Which screens a pair of switches says yes to.
fn screens(shown: Shown) -> &'static str {
    match (shown.dm, shown.tv) {
        (true, true) => text::history_screens_both(),
        (true, false) => text::history_screens_dm(),
        (false, true) => text::history_screens_tv(),
        (false, false) => text::history_screens_none(),
    }
}

/// Whether two numbers of the scene stand apart.
///
/// The history only asks which field a step wrote, so the smallest step a
/// number can take is a change.
fn differs(was: f64, now: f64) -> bool {
    (was - now).abs() > f64::EPSILON
}

/// How a step names the assets it touched: the file, or a count of them.
fn maps(assets: &[Asset]) -> String {
    match assets {
        [] => text::history_maps_one().to_owned(),
        [one] => file_name(one),
        many => text::history_maps_many(many.len()),
    }
}

/// The file name of a map, without the folders above it.
fn file_name(asset: &Asset) -> String {
    asset.path.file_name().map_or_else(
        || asset.path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// How a step names the group lists it rewrote.
fn lists(count: usize) -> String {
    if count == 1 {
        text::history_lists_one().to_owned()
    } else {
        text::history_lists_many(count)
    }
}

#[cfg(test)]
mod tests;
