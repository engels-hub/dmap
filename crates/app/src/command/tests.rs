//! The tests for the changes and the stack that walks back over them.
//!
//! A change and the history meet in every one of these, so they stand
//! here, over both submodules, and share the scene each one builds.

// Rust guideline compliant 2026-02-21

use super::{
    Both, Command as _, Deed, Grow, History, Restructure, SetAssets, SetName, SetShown, SetStrokes,
    SetTvBox, Turn, reshape,
};
use crate::scene::{Asset, Group, Node, ROOT_ID, Scene, Shown};
use crate::stroke::{Ink, Rule};

fn asset(id: u64, center: (f64, f64)) -> Asset {
    Asset {
        id,
        shown: Shown::default(),
        path: std::path::PathBuf::from("map.png"),
        center,
        grid_px: 50.0,
        rotation: 0.0,
        scale: 1.0,
        flip_x: false,
        flip_y: false,
        snap_offset: (0.0, 0.0),
    }
}

/// A scene with two maps in the root and one map inside a group.
fn scene() -> Scene {
    let mut scene = Scene::default();
    let mut group = Group::new(7, "Group 7".to_owned());
    group.children.push(Node::Asset(asset(3, (9.0, 9.0))));
    scene.root.children.push(Node::Asset(asset(1, (0.0, 0.0))));
    scene.root.children.push(Node::Asset(asset(2, (4.0, 0.0))));
    scene.root.children.push(Node::Group(group));
    scene
}

#[test]
fn a_move_of_two_maps_goes_back_in_one_step() {
    let mut scene = scene();
    let before = vec![asset(1, (0.0, 0.0)), asset(2, (4.0, 0.0))];
    let after = vec![asset(1, (1.0, 1.0)), asset(2, (5.0, 1.0))];
    let start = scene.clone();
    let mut history = History::default();
    history.run(&mut scene, SetAssets { before, after });
    assert_eq!(
        crate::scene::find(&scene, 1)
            .and_then(Node::asset)
            .unwrap()
            .center,
        (1.0, 1.0)
    );
    assert!(history.undo(&mut scene));
    assert_eq!(scene, start);
}

#[test]
fn a_drag_of_many_frames_is_one_step() {
    let mut scene = scene();
    let start = scene.clone();
    let mut history = History::default();
    for step in 1..=5 {
        let frame = f64::from(step);
        history.hold(
            &mut scene,
            SetAssets {
                before: vec![asset(1, (0.0, 0.0))],
                after: vec![asset(1, (frame, 0.0))],
            },
        );
    }
    history.settle();
    assert!(history.undo(&mut scene));
    assert_eq!(scene, start);
    assert!(!history.undo(&mut scene));
}

#[test]
fn redo_writes_the_change_again() {
    let mut scene = scene();
    let mut history = History::default();
    history.run(
        &mut scene,
        SetName {
            id: 7,
            before: "Group 7".to_owned(),
            after: "Cave".to_owned(),
        },
    );
    let named = scene.clone();
    assert!(history.undo(&mut scene));
    assert_ne!(scene, named);
    assert!(history.redo(&mut scene));
    assert_eq!(scene, named);
}

#[test]
fn a_new_change_drops_what_the_dm_took_back() {
    let mut scene = scene();
    let mut history = History::default();
    history.run(
        &mut scene,
        SetShown {
            id: 7,
            subject: "Group 7".to_owned(),
            before: Shown::default(),
            after: Shown {
                dm: true,
                tv: false,
            },
        },
    );
    assert!(history.undo(&mut scene));
    let box_now = scene.tv_box;
    history.run(
        &mut scene,
        SetTvBox {
            before: box_now,
            after: box_now,
        },
    );
    assert!(!history.redo(&mut scene));
}

#[test]
fn the_stack_holds_a_hundred_steps_and_no_more() {
    let mut scene = scene();
    let mut history = History::default();
    for step in 1..=super::history::STEPS + 10 {
        let frame = step as f64;
        history.run(
            &mut scene,
            SetAssets {
                before: vec![asset(1, (frame - 1.0, 0.0))],
                after: vec![asset(1, (frame, 0.0))],
            },
        );
    }
    let mut steps = 0;
    while history.undo(&mut scene) {
        steps += 1;
    }
    assert_eq!(steps, super::history::STEPS);
}

#[test]
fn a_turn_and_a_grow_put_every_asset_back_where_it_stood() {
    let mut scene = scene();
    let start = scene.clone();
    let starts = crate::scene::placed(&scene, &[1, 2]);
    let mut history = History::default();
    history.run(
        &mut scene,
        Turn {
            subject: "two maps".to_owned(),
            starts: starts.clone(),
            pivot: (2.0, 0.0),
            angle: 0.4,
        },
    );
    let turned = crate::scene::placed(&scene, &[1, 2]);
    history.run(
        &mut scene,
        Grow {
            subject: "two maps".to_owned(),
            starts: turned,
            pivot: (2.0, 0.0),
            factor: 1.5,
        },
    );
    assert_ne!(scene, start);
    assert!(history.undo(&mut scene));
    assert!(history.undo(&mut scene));
    assert_eq!(scene, start);
}

#[test]
fn a_delete_in_the_root_keeps_only_the_root_list() {
    let mut scene = scene();
    let start = scene.clone();
    let change = reshape(&mut scene, Deed::Ungroup, "the map".to_owned(), |scene| {
        crate::scene::take_node(scene, 2);
    })
    .unwrap();
    assert_eq!(change.before.len(), 1);
    assert_eq!(change.before[0].0, ROOT_ID);
    let mut history = History::default();
    history.kept(change);
    assert!(history.undo(&mut scene));
    assert_eq!(scene, start);
}

#[test]
fn a_move_inside_a_group_keeps_only_that_group() {
    let mut scene = scene();
    scene.root.children.push(Node::Asset(asset(4, (0.0, 6.0))));
    let start = scene.clone();
    let change = reshape(
        &mut scene,
        Deed::MoveInList,
        "the map".to_owned(),
        |scene| {
            crate::scene::move_into(scene, 4, 7);
        },
    )
    .unwrap();
    // The root list carries the group, so one list says it all.
    assert_eq!(change.after.len(), 1);
    assert_eq!(change.after[0].0, ROOT_ID);
    let mut history = History::default();
    history.kept(change);
    assert!(history.undo(&mut scene));
    assert_eq!(scene, start);
}

#[test]
fn an_order_change_inside_a_group_leaves_the_root_alone() {
    let mut scene = scene();
    if let Some(group) = crate::scene::group_mut(&mut scene, 7) {
        group.children.push(Node::Asset(asset(5, (9.0, 3.0))));
    }
    let start = scene.clone();
    let change = reshape(&mut scene, Deed::Order, "the map".to_owned(), |scene| {
        crate::scene::reorder_all(scene, &[3], true);
    })
    .unwrap();
    assert_eq!(change.after.len(), 1);
    assert_eq!(change.after[0].0, 7);
    let mut history = History::default();
    history.kept(change);
    assert!(history.undo(&mut scene));
    assert_eq!(scene, start);
}

#[test]
fn a_change_that_moves_nothing_is_no_step() {
    let mut scene = scene();
    // The group stands on top of the root already.
    assert!(
        reshape(
            &mut scene,
            Deed::MoveInList,
            "the map".to_owned(),
            |scene| {
                crate::scene::move_into(scene, 7, ROOT_ID);
            }
        )
        .is_none()
    );
}

#[test]
fn the_step_list_reads_oldest_first_and_keeps_what_was_taken_back() {
    let mut scene = scene();
    let mut history = History::default();
    history.run(
        &mut scene,
        SetAssets {
            before: vec![asset(1, (0.0, 0.0))],
            after: vec![asset(1, (1.0, 0.0))],
        },
    );
    history.run(
        &mut scene,
        SetName {
            id: 7,
            before: "Group 7".to_owned(),
            after: "Cave".to_owned(),
        },
    );
    let told: Vec<(String, String, String)> = history
        .steps()
        .into_iter()
        .map(|note| (note.what, note.subject, note.detail))
        .collect();
    assert_eq!(
        told,
        vec![
            (
                "Move".to_owned(),
                "map.png".to_owned(),
                "0.0, 0.0 to 1.0, 0.0 in".to_owned()
            ),
            (
                "Rename".to_owned(),
                "Group 7".to_owned(),
                "to Cave".to_owned()
            ),
        ]
    );
    assert_eq!(history.steps()[0].ago, "just now");
    assert_eq!(history.place(), 2);
    assert!(history.undo(&mut scene));
    // The step the DM took back stays on the list, behind the place.
    assert_eq!(history.steps().len(), 2);
    assert_eq!(history.place(), 1);
}

#[test]
fn a_step_says_how_long_ago_the_dm_made_it() {
    let now = 1_000_000;
    assert_eq!(super::ago(0, now), "earlier");
    assert_eq!(super::ago(now, now), "just now");
    assert_eq!(super::ago(now - 44, now), "just now");
    assert_eq!(super::ago(now - 45, now), "1 min ago");
    assert_eq!(super::ago(now - 600, now), "10 min ago");
    assert_eq!(super::ago(now - 5400, now), "2 h ago");
    assert_eq!(super::ago(now - 8 * 3600, now), "8 h ago");
    assert_eq!(super::ago(now - 86_400, now), "a day ago");
    assert_eq!(super::ago(now - 40 * 3600, now), "a day ago");
    assert_eq!(super::ago(now - 3 * 86_400, now), "3 days ago");
    // A clock that went back leaves a step in the future.
    assert_eq!(super::ago(now + 60, now), "just now");
}

#[test]
fn a_walk_goes_to_the_step_the_dm_picked() {
    let mut scene = scene();
    let start = scene.clone();
    let mut history = History::default();
    for step in 1..=4 {
        let frame = f64::from(step);
        history.run(
            &mut scene,
            SetAssets {
                before: vec![asset(1, (frame - 1.0, 0.0))],
                after: vec![asset(1, (frame, 0.0))],
            },
        );
    }
    let all = scene.clone();
    assert!(history.walk_to(&mut scene, 0));
    assert_eq!(scene, start);
    assert_eq!(history.place(), 0);
    assert!(history.walk_to(&mut scene, 4));
    assert_eq!(scene, all);
    // The place it stands on already asks for no walk.
    assert!(!history.walk_to(&mut scene, 4));
}

#[test]
fn a_stack_that_wrote_itself_asks_for_no_second_write() {
    let mut scene = scene();
    let mut history = History::default();
    assert!(!history.unwritten());
    history.run(
        &mut scene,
        SetName {
            id: 7,
            before: "Group 7".to_owned(),
            after: "Cave".to_owned(),
        },
    );
    assert!(history.unwritten());
    history.wrote();
    assert!(!history.unwritten());
    assert!(history.undo(&mut scene));
    assert!(history.unwritten());
    // A stack that came from a file is the file.
    let back = History::from_json(&history.to_json()).unwrap();
    assert!(!back.unwritten());
}

/// A drawing of two points, for the changes a drag makes. Issue #69.
fn stroke(id: u64, points: &[(f64, f64)]) -> crate::stroke::Stroke {
    crate::stroke::Stroke {
        id,
        shown: Shown::default(),
        ink: Ink::Line,
        points: points.to_vec(),
        color: [0, 0, 0, 255],
        width: 0.1,
        span: 0.0,
        angle: 0.0,
        rule: Rule::default(),
    }
}

#[test]
fn a_drag_of_a_map_and_a_drawing_is_one_step() {
    let mut scene = scene();
    let mark = stroke(5, &[(0.0, 0.0), (1.0, 0.0)]);
    scene.root.children.push(Node::Stroke(mark.clone()));
    let was = crate::scene::find(&scene, 1)
        .unwrap()
        .asset()
        .unwrap()
        .clone();
    let mut moved = was.clone();
    moved.center = (2.0, 2.0);
    let mut history = History::default();
    history.run(
        &mut scene,
        Both::of(
            SetAssets {
                before: vec![was.clone()],
                after: vec![moved],
            },
            SetStrokes {
                before: vec![mark.clone()],
                after: vec![mark.moved((2.0, 2.0))],
            },
        ),
    );
    assert_eq!(
        crate::scene::find(&scene, 1)
            .unwrap()
            .asset()
            .unwrap()
            .center,
        (2.0, 2.0)
    );
    assert_eq!(
        crate::scene::find(&scene, 5)
            .unwrap()
            .stroke()
            .unwrap()
            .points,
        vec![(2.0, 2.0), (3.0, 2.0)]
    );
    // One gesture is one step, so one undo puts both back.
    assert_eq!(history.steps().len(), 1);
    assert!(history.undo(&mut scene));
    assert_eq!(
        crate::scene::find(&scene, 1)
            .unwrap()
            .asset()
            .unwrap()
            .center,
        was.center
    );
    assert_eq!(
        crate::scene::find(&scene, 5)
            .unwrap()
            .stroke()
            .unwrap()
            .points,
        mark.points
    );
}

#[test]
fn the_list_says_what_a_drag_did_to_a_drawing() {
    let mark = stroke(5, &[(0.0, 0.0), (2.0, 0.0)]);
    // A move carries the middle.
    let moved = SetStrokes {
        before: vec![mark.clone()],
        after: vec![mark.moved((3.0, 0.0))],
    };
    assert_eq!(moved.note().what, crate::text::history_deed_move());
    // A turn of a box goes into the angle.
    let mut turned = mark.clone();
    turned.ink = Ink::Rect;
    let spun = SetStrokes {
        before: vec![turned.clone()],
        after: vec![turned.turned((1.0, 0.0), std::f64::consts::FRAC_PI_2)],
    };
    assert_eq!(spun.note().what, crate::text::history_deed_turn());
    // A growth takes the width with it, so the list says size.
    let grown = SetStrokes {
        before: vec![mark.clone()],
        after: vec![mark.scaled((0.0, 0.0), 2.0)],
    };
    assert_eq!(grown.note().what, crate::text::history_deed_size());
}

#[test]
fn a_delete_takes_a_group_and_brings_it_back_whole() {
    let mut scene = scene();
    let mut history = History::default();
    let change = reshape(&mut scene, Deed::Delete, "Group 7".to_owned(), |scene| {
        crate::scene::take_node(scene, 7);
    })
    .expect("the group left the tree");
    history.kept(change);
    assert!(crate::scene::find(&scene, 7).is_none());
    // The map inside the group went with it.
    assert!(crate::scene::find(&scene, 3).is_none());
    assert!(history.undo(&mut scene));
    assert!(crate::scene::find(&scene, 7).is_some());
    assert!(crate::scene::find(&scene, 3).is_some());
    // And it came back where it sat: after the two maps of the root.
    assert_eq!(scene.root.children.len(), 3);
    assert_eq!(scene.root.children[2].id(), 7);
}

#[test]
fn a_stack_comes_back_from_its_file() {
    let mut scene = scene();
    let start = scene.clone();
    let mut history = History::default();
    history.run(
        &mut scene,
        SetAssets {
            before: vec![asset(1, (0.0, 0.0))],
            after: vec![asset(1, (2.0, 0.0))],
        },
    );
    history.run(
        &mut scene,
        SetName {
            id: 7,
            before: "Group 7".to_owned(),
            after: "Cave".to_owned(),
        },
    );
    let change = reshape(&mut scene, Deed::Ungroup, "the map".to_owned(), |scene| {
        crate::scene::take_node(scene, 2);
    })
    .unwrap();
    history.kept(change);
    // One step the DM took back must come back as one they can write
    // again, not as one that is gone.
    assert!(history.undo(&mut scene));
    let written = history.to_json();
    let saved = scene.clone();

    let mut back = History::from_json(&written).unwrap();
    assert_eq!(back.steps(), history.steps());
    assert_eq!(back.steps()[2].what, "Ungroup");
    assert_eq!(back.place(), 2);
    assert!(back.redo(&mut scene));
    assert!(back.walk_to(&mut scene, 0));
    assert_eq!(scene, start);
    assert!(back.walk_to(&mut scene, 2));
    assert_eq!(scene, saved);
}

#[test]
fn a_file_that_says_more_steps_than_it_holds_stops_at_the_end() {
    let mut scene = scene();
    let history = History::from_json(r#"{"place":9,"steps":[]}"#).unwrap();
    assert_eq!(history.place(), 0);
    assert!(history.steps().is_empty());
    let mut history = history;
    assert!(!history.undo(&mut scene));
}

#[test]
fn a_scene_folder_with_no_history_file_reads_as_an_empty_stack() {
    let history = History::from_json("{}").unwrap();
    assert_eq!(history.place(), 0);
    assert!(history.steps().is_empty());
}

#[test]
fn a_restructure_puts_a_whole_list_back() {
    let mut scene = scene();
    let before = scene.root.children.clone();
    let change = Restructure {
        what: Deed::Ungroup,
        subject: "the map".to_owned(),
        before: vec![(ROOT_ID, before.clone())],
        after: vec![(ROOT_ID, Vec::new())],
    };
    change.apply(&mut scene);
    assert!(scene.root.children.is_empty());
    change.revert(&mut scene);
    assert_eq!(scene.root.children, before);
}
