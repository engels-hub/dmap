//! The stack of steps, and the file it comes back from.
//!
//! The history writes every change and keeps it, so `Ctrl+Z` puts the
//! scene back and `Ctrl+Shift+Z` writes it again. A change under the
//! DM's hand comes in once a frame through `hold`, so a drag of a
//! hundred frames is one step. PLAN.md 5.1.

// Rust guideline compliant 2026-02-21

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::scene::Scene;

use super::change::Change;
use super::{Command, Note, ago, now};

/// How many changes the stack keeps.
///
/// A step holds the values it rewrote, and a step that reshapes the tree
/// holds the children of the group it reshaped. A hundred of those is
/// megabytes at worst, and further back than a DM ever reaches in one
/// session.
pub(super) const STEPS: usize = 100;

/// One step: the change the DM made, and when they made it.
#[derive(Debug, Serialize, Deserialize)]
struct Step {
    /// What the step writes and takes back.
    change: Change,
    /// The seconds since the epoch. A file from an older version of the
    /// program carries none, and the step reads as `earlier`.
    #[serde(default)]
    at: u64,
}

/// The stack as `history.json` gives it back.
#[derive(Debug, Default, Deserialize)]
struct Read {
    /// How many of the steps stand written. The rest the DM took back.
    #[serde(default)]
    place: usize,
    /// Every step, oldest first.
    #[serde(default)]
    steps: Vec<Step>,
}

/// The same, on the way out, so no step is cloned to be written.
#[derive(Debug, Serialize)]
struct Written<'a> {
    place: usize,
    steps: Vec<&'a Step>,
}

/// The changes the DM made, and the ones they took back.
///
/// The stack keeps [`STEPS`] changes. An older one falls off the bottom.
/// It lives beside the scene in `history.json`, so the DM closes the
/// program and undoes yesterday's work tomorrow. A new scene brings its
/// own stack.
#[derive(Debug, Default)]
pub struct History {
    /// What the DM did, oldest first.
    done: VecDeque<Step>,
    /// What the DM took back, the last one last.
    undone: Vec<Step>,
    /// The change under the DM's hand, which is not a step yet.
    open: Option<Step>,
    /// Whether the stack changed since the last write to `history.json`.
    ///
    /// A step carries the values it wrote, and a step that reshaped the
    /// tree carries branches of it, so the file grows with the scene. A
    /// save that has nothing new to say writes nothing.
    unwritten: bool,
}

impl History {
    /// Writes a change that the DM is still making.
    ///
    /// The history drops the change it held and keeps this one, so a drag
    /// is one step. Every frame of the drag must carry the same `before`,
    /// because that is the state the undo goes back to.
    pub fn hold(&mut self, scene: &mut Scene, change: impl Into<Change>) {
        let change = change.into();
        change.apply(scene);
        self.undone.clear();
        // The time of a drag is the time it ends, because every frame of
        // it writes this again.
        self.open = Some(Step { change, at: now() });
        self.unwritten = true;
    }

    /// Closes the change [`History::hold`] wrote, once the drag ends.
    pub fn settle(&mut self) {
        let Some(change) = self.open.take() else {
            return;
        };
        self.done.push_back(change);
        if self.done.len() > STEPS {
            self.done.pop_front();
        }
        self.unwritten = true;
    }

    /// Writes a change that is over in one frame.
    ///
    /// A change the DM still had under their hand becomes a step of its
    /// own first, so a press of a button never swallows the drag before it.
    pub fn run(&mut self, scene: &mut Scene, change: impl Into<Change>) {
        self.settle();
        self.hold(scene, change);
        self.settle();
    }

    /// Keeps a change that is in the scene already, as one step.
    ///
    /// [`reshape`](super::reshape) writes the tree while it reads what
    /// moved, so its result comes in here.
    pub fn kept(&mut self, change: impl Into<Change>) {
        self.settle();
        self.undone.clear();
        self.open = Some(Step {
            change: change.into(),
            at: now(),
        });
        self.settle();
    }

    /// Whether a change is under the DM's hand.
    pub fn holding(&self) -> bool {
        self.open.is_some()
    }

    /// Takes the last change back. Returns `true` when it did.
    pub fn undo(&mut self, scene: &mut Scene) -> bool {
        let Some(step) = self.done.pop_back() else {
            return false;
        };
        step.change.revert(scene);
        self.undone.push(step);
        self.unwritten = true;
        true
    }

    /// Writes the last change the DM took back. Returns `true` when it did.
    pub fn redo(&mut self, scene: &mut Scene) -> bool {
        let Some(step) = self.undone.pop() else {
            return false;
        };
        step.change.apply(scene);
        self.done.push_back(step);
        self.unwritten = true;
        true
    }

    /// What every step says, oldest first. DESIGN.md 9.8.
    ///
    /// The steps past [`History::place`] are the ones the DM took back.
    pub fn steps(&self) -> Vec<Note> {
        let now = now();
        self.done
            .iter()
            .chain(self.undone.iter().rev())
            .map(|step| Note {
                ago: ago(step.at, now),
                ..step.change.note()
            })
            .collect()
    }

    /// How many steps stand written. The rest the DM took back.
    pub fn place(&self) -> usize {
        self.done.len()
    }

    /// Walks the scene to the state after `place` steps.
    ///
    /// Returns `true` when the scene moved. A place past the end of the
    /// stack walks as far as the stack goes.
    pub fn walk_to(&mut self, scene: &mut Scene, place: usize) -> bool {
        let mut moved = false;
        while self.done.len() > place && self.undo(scene) {
            moved = true;
        }
        while self.done.len() < place && self.redo(scene) {
            moved = true;
        }
        moved
    }

    /// Whether the stack has something the file beside the scene lacks.
    pub fn unwritten(&self) -> bool {
        self.unwritten
    }

    /// Marks the stack as written, once the file holds it.
    pub fn wrote(&mut self) {
        self.unwritten = false;
    }

    /// The stack as JSON, for the file beside the scene.
    ///
    /// The change under the DM's hand is left out. It is no step yet, and
    /// the program writes the files only once the DM lets go.
    ///
    /// The JSON holds no line breaks. A step carries every value it wrote,
    /// and a step that reshaped the tree carries whole branches of it, so
    /// a hundred pretty-printed steps would make a file no one reads
    /// anyway large.
    pub fn to_json(&self) -> String {
        let file = Written {
            place: self.done.len(),
            steps: self.done.iter().chain(self.undone.iter().rev()).collect(),
        };
        serde_json::to_string(&file).expect("a change has no unserializable field")
    }

    /// Reads a stack that a run before this one wrote.
    ///
    /// # Errors
    ///
    /// Returns an error when the JSON does not hold a stack.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let file: Read = serde_json::from_str(json)?;
        let mut steps = file.steps;
        let place = file.place.min(steps.len());
        let taken_back: Vec<Step> = steps.split_off(place).into_iter().rev().collect();
        Ok(Self {
            done: steps.into(),
            undone: taken_back,
            open: None,
            unwritten: false,
        })
    }
}
