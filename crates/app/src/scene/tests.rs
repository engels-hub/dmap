//! The tests for the scene: its tree, its order, its files.
//!
//! A scene is one tree, so these tests build one and ask every part of
//! the module about it. They stand here, over the submodules they share.

// Rust guideline compliant 2026-02-21

use std::path::{Path, PathBuf};

use super::files::free_name;
use super::tree::path_of;
use super::{
    Asset, Audience, Group, Node, NodeId, ROOT_ID, Scene, assets, assets_of, copy_into_scene,
    dm_draw_order, draw_order, find, group_selection, move_above, normalize, parent_of, path_to,
};

/// A folder of its own for one test, under the system's temp folder.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dmap-test-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
use crate::tvbox::TvBox;

fn close(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
}

/// A scene with one map loose under the root and one inside a group.
fn scene_with_a_group() -> Scene {
    let mut scene = Scene::default();
    scene.root.children.push(Node::Asset(Asset::new(
        1,
        PathBuf::from("loose.png"),
        (0.0, 0.0),
    )));
    let mut notes = Group::new(2, "Notes".to_owned());
    notes.children.push(Node::Asset(Asset::new(
        3,
        PathBuf::from("secret.png"),
        (0.0, 0.0),
    )));
    scene.root.children.push(Node::Group(notes));
    scene
}

/// The name of each asset the DM draws, with whether the TV shows it.
fn dm_names(scene: &Scene) -> Vec<(String, bool)> {
    dm_draw_order(scene)
        .into_iter()
        .map(|(asset, on_tv)| (asset.path.to_string_lossy().into_owned(), on_tv))
        .collect()
}

#[test]
fn the_path_to_the_root_is_the_root_alone() {
    let scene = scene_with_a_group();
    let path = path_to(&scene, ROOT_ID);
    assert_eq!(path.len(), 1);
    assert_eq!(path[0].0, ROOT_ID);
}

#[test]
fn the_path_to_a_group_starts_at_the_root_and_ends_at_the_group() {
    let scene = scene_with_a_group();
    let names: Vec<String> = path_to(&scene, 2).into_iter().map(|(_, n)| n).collect();
    assert_eq!(names.len(), 2);
    assert_eq!(names[1], "Notes");
    assert_eq!(path_to(&scene, 2)[0].0, ROOT_ID);
}

#[test]
fn a_path_to_a_group_that_went_is_empty() {
    // The panel falls back to the root when its group is gone.
    let scene = scene_with_a_group();
    assert!(path_to(&scene, 99).is_empty());
    // An asset is not a group, so it names no path either.
    assert!(path_to(&scene, 3).is_empty());
}

#[test]
fn the_dm_sees_every_map_and_which_ones_reach_the_tv() {
    let scene = scene_with_a_group();
    assert_eq!(
        dm_names(&scene),
        vec![
            ("loose.png".to_owned(), true),
            ("secret.png".to_owned(), true)
        ]
    );
}

#[test]
fn a_map_off_the_tv_still_draws_for_the_dm() {
    // DESIGN.md 5.6: the DM keeps it, at half strength.
    let mut scene = scene_with_a_group();
    let Some(Node::Asset(loose)) = scene.root.children.first_mut() else {
        panic!("the first child is the loose map");
    };
    loose.shown.tv = false;
    assert_eq!(
        dm_names(&scene),
        vec![
            ("loose.png".to_owned(), false),
            ("secret.png".to_owned(), true)
        ]
    );
}

#[test]
fn a_group_off_the_tv_takes_its_children_off_the_tv() {
    let mut scene = scene_with_a_group();
    let Some(Node::Group(notes)) = scene.root.children.get_mut(1) else {
        panic!("the second child is the group");
    };
    notes.shown.tv = false;
    // The asset inside says yes, and the group over it still wins.
    assert_eq!(
        dm_names(&scene),
        vec![
            ("loose.png".to_owned(), true),
            ("secret.png".to_owned(), false)
        ]
    );
}

#[test]
fn a_map_off_the_dm_screen_leaves_the_dm_order() {
    // Half strength belongs to the TV switch. The DM switch still hides.
    let mut scene = scene_with_a_group();
    let Some(Node::Asset(loose)) = scene.root.children.first_mut() else {
        panic!("the first child is the loose map");
    };
    loose.shown.dm = false;
    assert_eq!(dm_names(&scene), vec![("secret.png".to_owned(), true)]);
}

#[test]
fn the_dm_order_and_the_tv_order_hold_the_same_maps_when_both_show() {
    let scene = scene_with_a_group();
    let tv: Vec<String> = draw_order(&scene, Audience::Tv)
        .into_iter()
        .map(|asset| asset.path.to_string_lossy().into_owned())
        .collect();
    let dm: Vec<String> = dm_names(&scene).into_iter().map(|(name, _)| name).collect();
    assert_eq!(dm, tv);
}

#[test]
fn a_map_spans_its_pixels_divided_by_pixels_per_cell() {
    let map = Asset {
        grid_px: 50.0,
        ..Asset::new(0, PathBuf::from("crypt.png"), (10.0, 5.0))
    };
    let corners = map.corners((400, 300));
    assert!(close(corners[0], (6.0, 2.0)));
    assert!(close(corners[2], (14.0, 8.0)));
}

#[test]
fn corners_of_an_unrotated_map_are_its_rect() {
    let map = Asset::new(0, PathBuf::from("m.png"), (10.0, 5.0));
    let corners = map.corners((400, 300));
    assert!(close(corners[0], (8.0, 3.5)));
    assert!(close(corners[1], (12.0, 3.5)));
    assert!(close(corners[2], (12.0, 6.5)));
    assert!(close(corners[3], (8.0, 6.5)));
}

#[test]
fn a_quarter_turn_swaps_width_and_height() {
    let mut map = Asset::new(0, PathBuf::from("m.png"), (0.0, 0.0));
    map.rotation = std::f64::consts::FRAC_PI_2;
    let corners = map.corners((400, 300));
    // The image's top-left corner moves to the top-right of the turned map.
    assert!(close(corners[0], (1.5, -2.0)));
    assert!(close(corners[1], (1.5, 2.0)));
}

#[test]
fn scale_grows_the_map_around_its_center() {
    let mut map = Asset::new(0, PathBuf::from("m.png"), (0.0, 0.0));
    map.scale = 2.0;
    let corners = map.corners((400, 300));
    assert!(close(corners[0], (-4.0, -3.0)));
    assert!(close(corners[2], (4.0, 3.0)));
}

#[test]
fn old_project_files_without_transform_fields_still_load() {
    let json = r#"{"path":"m.png","center":[1.0,2.0],"grid_px":50.0}"#;
    let map: Asset = serde_json::from_str(json).unwrap();
    assert!((map.rotation).abs() < 1e-9);
    assert!((map.scale - 1.0).abs() < 1e-9);
    assert!(!map.flip_x && !map.flip_y);
    // A map from before this field snaps to the canvas grid, as it did.
    assert_eq!(map.snap_offset, (0.0, 0.0));
}

#[test]
fn a_new_map_uses_the_default_pixels_per_cell() {
    let map = Asset::new(0, PathBuf::from("crypt.png"), (0.0, 0.0));
    assert!((map.grid_px - Asset::DEFAULT_GRID_PX).abs() < 1e-9);
}

#[test]
fn a_scene_round_trips_through_json() {
    let mut scene = Scene {
        tv_box: TvBox {
            center: (3.0, -1.5),
            width: 36.0,
        },
        ..Scene::default()
    };
    let asset = Asset {
        grid_px: 140.0,
        rotation: 0.5,
        scale: 1.5,
        flip_x: true,
        snap_offset: (0.25, 0.75),
        ..Asset::new(1, PathBuf::from("crypt.png"), (1.5, -2.0))
    };
    let mut notes = Group::new(2, "Notes".to_owned());
    notes.shown.tv = false;
    notes.children.push(Node::Asset(Asset::new(
        3,
        PathBuf::from("note.png"),
        (0.0, 0.0),
    )));
    scene.root.children.push(Node::Asset(asset));
    scene.root.children.push(Node::Group(notes));
    assert_eq!(Scene::from_json(&scene.to_json()).unwrap(), scene);
}

#[test]
fn an_empty_scene_file_gives_an_empty_scene() {
    assert_eq!(Scene::from_json("{}").unwrap(), Scene::default());
}

#[test]
fn a_scene_holds_the_names_of_files_beside_it() {
    let mut scene = Scene::default();
    scene.root.children.push(Node::Asset(Asset::new(
        1,
        PathBuf::from("crypt.png"),
        (0.0, 0.0),
    )));
    assert!(scene.to_json().contains("\"crypt.png\""));
    // The file says what each node is, so a DM can read the tree.
    assert!(scene.to_json().contains("\"type\": \"asset\""));
}

#[test]
fn a_map_keeps_its_own_name_in_an_empty_folder() {
    assert_eq!(
        free_name(Path::new("/maps/crypt.png"), |_| false),
        "crypt.png"
    );
}

#[test]
fn another_file_of_the_same_name_gets_a_number() {
    let taken = |name: &str| name == "crypt.png";
    assert_eq!(
        free_name(Path::new("/maps/crypt.png"), taken),
        "crypt 2.png"
    );
    let taken = |name: &str| name == "crypt.png" || name == "crypt 2.png";
    assert_eq!(
        free_name(Path::new("/maps/crypt.png"), taken),
        "crypt 3.png"
    );
}

#[test]
fn a_file_with_no_extension_still_gets_a_name() {
    let taken = |name: &str| name == "crypt";
    assert_eq!(free_name(Path::new("/maps/crypt"), taken), "crypt 2");
}

#[test]
fn a_map_from_elsewhere_is_copied_into_the_scene() {
    let scene = scratch("copy");
    let outside = scene.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let source = outside.join("crypt.png");
    std::fs::write(&source, b"first map").unwrap();

    let name = copy_into_scene(&scene, &source).unwrap();
    assert_eq!(name, PathBuf::from("crypt.png"));
    assert_eq!(
        std::fs::read(scene.join("crypt.png")).unwrap(),
        b"first map"
    );
}

#[test]
fn the_same_map_twice_keeps_one_copy() {
    let scene = scratch("twice");
    let source = scene.join("far/crypt.png");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, b"first map").unwrap();

    assert_eq!(
        copy_into_scene(&scene, &source).unwrap(),
        PathBuf::from("crypt.png")
    );
    assert_eq!(
        copy_into_scene(&scene, &source).unwrap(),
        PathBuf::from("crypt.png")
    );
    let copies = std::fs::read_dir(&scene)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|end| end == "png"))
        .count();
    assert_eq!(copies, 1);
}

#[test]
fn a_different_map_of_the_same_name_keeps_both() {
    let scene = scratch("clash");
    let one = scene.join("a/crypt.png");
    let two = scene.join("b/crypt.png");
    for (path, bytes) in [(&one, b"first map"), (&two, b"other map")] {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    assert_eq!(
        copy_into_scene(&scene, &one).unwrap(),
        PathBuf::from("crypt.png")
    );
    assert_eq!(
        copy_into_scene(&scene, &two).unwrap(),
        PathBuf::from("crypt 2.png")
    );
    assert_eq!(
        std::fs::read(scene.join("crypt.png")).unwrap(),
        b"first map"
    );
    assert_eq!(
        std::fs::read(scene.join("crypt 2.png")).unwrap(),
        b"other map"
    );
}

#[test]
fn a_map_already_in_the_scene_folder_is_left_alone() {
    let scene = scratch("inside");
    let source = scene.join("crypt.png");
    std::fs::write(&source, b"first map").unwrap();
    assert_eq!(
        copy_into_scene(&scene, &source).unwrap(),
        PathBuf::from("crypt.png")
    );
    assert_eq!(std::fs::read_dir(&scene).unwrap().count(), 1);
}

/// A root with an asset, a group, and an asset inside the group.
fn nested() -> Scene {
    let mut scene = Scene::default();
    let mut notes = Group::new(2, "Notes".to_owned());
    notes.children.push(Node::Asset(Asset::new(
        3,
        PathBuf::from("note.png"),
        (0.0, 0.0),
    )));
    scene.root.children.push(Node::Asset(Asset::new(
        1,
        PathBuf::from("floor.png"),
        (0.0, 0.0),
    )));
    scene.root.children.push(Node::Group(notes));
    scene
}

#[test]
fn a_group_draws_inside_the_place_it_holds() {
    let scene = nested();
    let order: Vec<_> = draw_order(&scene, Audience::Dm)
        .iter()
        .map(|asset| asset.id)
        .collect();
    assert_eq!(order, vec![1, 3]);
}

#[test]
fn a_group_that_is_off_takes_its_children_off() {
    let mut scene = nested();
    let Some(Node::Group(notes)) = scene.root.children.get_mut(1) else {
        panic!("the second child is the group");
    };
    notes.shown.tv = false;
    assert_eq!(draw_order(&scene, Audience::Dm).len(), 2);
    assert_eq!(draw_order(&scene, Audience::Tv).len(), 1);
}

#[test]
fn an_asset_can_hide_on_its_own() {
    let mut scene = nested();
    let Some(Node::Asset(floor)) = scene.root.children.get_mut(0) else {
        panic!("the first child is the asset");
    };
    floor.shown.tv = false;
    assert_eq!(draw_order(&scene, Audience::Tv)[0].id, 3);
}

#[test]
fn a_scene_from_before_the_tree_opens_under_the_root() {
    let json = r#"{"maps":[
        {"path":"a.png","center":[0.0,0.0],"grid_px":50.0},
        {"path":"b.png","center":[1.0,1.0],"grid_px":50.0}],
        "tv_box":{"center":[0.0,0.0],"width":40.0}}"#;
    let scene = Scene::from_json(json).unwrap();
    assert_eq!(scene.root.children.len(), 2);
    assert_eq!(assets(&scene).len(), 2);
    assert!((scene.tv_box.width - 40.0).abs() < 1e-9);
    // Every asset took a name of its own.
    let ids: Vec<_> = assets(&scene).iter().map(|asset| asset.id).collect();
    assert_eq!(ids, vec![1, 2]);
}

#[test]
fn a_file_with_two_nodes_of_one_name_gets_them_apart() {
    let json = r#"{"root":{"id":0,"name":"Scene","shown":{"dm":true,"tv":true},"children":[
        {"type":"asset","id":5,"path":"a.png","center":[0.0,0.0],"grid_px":50.0},
        {"type":"asset","id":5,"path":"b.png","center":[0.0,0.0],"grid_px":50.0}]}}"#;
    let scene = Scene::from_json(json).unwrap();
    let ids: Vec<_> = assets(&scene).iter().map(|asset| asset.id).collect();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
}

#[test]
fn the_root_keeps_its_name_whatever_the_file_says() {
    let json = r#"{"root":{"id":9,"name":"Not the root","shown":{"dm":false,"tv":false},
        "children":[]}}"#;
    let scene = Scene::from_json(json).unwrap();
    assert_eq!(scene.root.id, ROOT_ID);
    assert_eq!(scene.root.name, "Scene");
    assert!(scene.root.shown.dm && scene.root.shown.tv);
}

#[test]
fn a_new_name_is_one_no_node_holds() {
    let scene = nested();
    assert_eq!(scene.next_id(), 4);
}

#[test]
fn a_node_moves_among_the_nodes_it_sits_with() {
    let mut scene = nested();
    // The asset at the bottom of the root moves over the group.
    assert!(super::reorder_all(&mut scene, &[1], true));
    assert_eq!(scene.root.children[0].id(), 2);
    assert_eq!(scene.root.children[1].id(), 1);
    // It cannot go any further.
    assert!(!super::reorder_all(&mut scene, &[1], true));
}

#[test]
fn a_node_inside_a_group_moves_inside_that_group() {
    let mut scene = nested();
    // The only child of the group has nowhere to go.
    assert!(!super::reorder_all(&mut scene, &[3], true));
    assert!(!super::reorder_all(&mut scene, &[3], false));
    assert_eq!(parent_of(&scene, 3), Some((2, 0)));
}

#[test]
fn the_tree_says_which_group_holds_a_node() {
    let scene = nested();
    assert_eq!(parent_of(&scene, 1), Some((ROOT_ID, 0)));
    assert_eq!(parent_of(&scene, 2), Some((ROOT_ID, 1)));
    assert_eq!(parent_of(&scene, 3), Some((2, 0)));
    assert_eq!(parent_of(&scene, 9), None);
    assert!(find(&scene, 3).is_some());
    assert!(find(&scene, 9).is_none());
}

/// A root with two assets, and a group holding two more.
///
/// ```text
/// root
///   1 floor.png
///   2 wall.png
///   3 Notes
///       4 note.png
///       5 pin.png
/// ```
fn family() -> Scene {
    let mut scene = Scene::default();
    let mut notes = Group::new(3, "Notes".to_owned());
    for (id, name) in [(4, "note.png"), (5, "pin.png")] {
        notes
            .children
            .push(Node::Asset(Asset::new(id, PathBuf::from(name), (0.0, 0.0))));
    }
    for (id, name) in [(1, "floor.png"), (2, "wall.png")] {
        scene
            .root
            .children
            .push(Node::Asset(Asset::new(id, PathBuf::from(name), (0.0, 0.0))));
    }
    scene.root.children.push(Node::Group(notes));
    scene
}

#[test]
fn a_path_says_where_a_node_sits() {
    let scene = family();
    assert_eq!(path_of(&scene, 1), Some(vec![0]));
    assert_eq!(path_of(&scene, 3), Some(vec![2]));
    assert_eq!(path_of(&scene, 5), Some(vec![2, 1]));
    assert_eq!(path_of(&scene, 9), None);
}

#[test]
fn a_selection_drops_what_a_group_in_it_already_holds() {
    let scene = family();
    // The group and one of its children: the group carries it.
    assert_eq!(normalize(&scene, &[3, 4]), vec![3]);
    assert_eq!(normalize(&scene, &[4, 3]), vec![3]);
    // Two nodes that hold each other, and one that holds neither.
    assert_eq!(normalize(&scene, &[1, 3, 5]), vec![1, 3]);
    // The same node twice is one node.
    assert_eq!(normalize(&scene, &[1, 1]), vec![1]);
}

#[test]
fn a_group_holds_every_asset_under_it() {
    let scene = family();
    assert_eq!(assets_of(&scene, 1), vec![1]);
    assert_eq!(assets_of(&scene, 3), vec![4, 5]);
    assert!(assets_of(&scene, 9).is_empty());
}

#[test]
fn two_brothers_group_where_they_stood() {
    let mut scene = family();
    let id = group_selection(&mut scene, &[1, 2], "New".to_owned()).unwrap();
    // The root holds the new group and the old group, nothing else.
    assert_eq!(scene.root.children.len(), 2);
    assert_eq!(scene.root.children[0].id(), id);
    assert_eq!(scene.root.children[1].id(), 3);
    let Some(Node::Group(new)) = scene.root.children.first() else {
        panic!("the first child is the new group");
    };
    // They keep the order they drew in.
    assert_eq!(
        new.children.iter().map(Node::id).collect::<Vec<_>>(),
        vec![1, 2]
    );
}

#[test]
fn an_uncle_and_a_nephew_group_in_the_root() {
    let mut scene = family();
    // floor.png sits in the root, pin.png sits in Notes.
    let id = group_selection(&mut scene, &[1, 5], "New".to_owned()).unwrap();
    assert_eq!(
        path_of(&scene, 1),
        Some(vec![path_of(&scene, id).unwrap()[0], 0])
    );
    // The nephew left Notes, which keeps its other child.
    let Some(Node::Group(notes)) = scene.root.children.iter().find(|n| n.id() == 3) else {
        panic!("Notes is still in the root");
    };
    assert_eq!(notes.children.len(), 1);
    assert_eq!(notes.children[0].id(), 4);
}

#[test]
fn two_children_of_one_group_stay_in_that_group() {
    let mut scene = family();
    let id = group_selection(&mut scene, &[4, 5], "New".to_owned()).unwrap();
    // The new group sits inside Notes, since Notes holds them both.
    assert_eq!(path_of(&scene, id), Some(vec![2, 0]));
    assert_eq!(assets_of(&scene, id), vec![4, 5]);
}

#[test]
fn a_group_and_its_own_child_group_as_one() {
    let mut scene = family();
    // The group carries the child, so this groups the group alone.
    let id = group_selection(&mut scene, &[3, 4], "New".to_owned()).unwrap();
    assert_eq!(assets_of(&scene, id), vec![4, 5]);
    assert_eq!(scene.root.children.len(), 3);
}

#[test]
fn a_group_of_nothing_is_no_group() {
    let mut scene = family();
    assert_eq!(group_selection(&mut scene, &[], "New".to_owned()), None);
    assert_eq!(group_selection(&mut scene, &[9], "New".to_owned()), None);
}

#[test]
fn a_group_draws_where_its_highest_member_drew() {
    let mut scene = family();
    // wall.png draws over floor.png, and Notes draws over both.
    let id = group_selection(&mut scene, &[2, 3], "New".to_owned()).unwrap();
    // The new group takes the place Notes had, over floor.png.
    assert_eq!(
        scene.root.children.iter().map(Node::id).collect::<Vec<_>>(),
        vec![1, id]
    );
}

#[test]
fn a_new_group_takes_a_name_no_member_holds() {
    let mut scene = family();
    let id = group_selection(&mut scene, &[1, 2], "New".to_owned()).unwrap();
    // The members leave the tree while the group is made, so a name
    // counted then would land on one of them.
    assert!(!assets_of(&scene, id).contains(&id));
    let mut names: Vec<NodeId> = Vec::new();
    let mut walk: Vec<&Node> = scene.root.children.iter().collect();
    while let Some(node) = walk.pop() {
        names.push(node.id());
        if let Node::Group(group) = node {
            walk.extend(group.children.iter());
        }
    }
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        names.len(),
        "two nodes share a name: {names:?}"
    );
}

#[test]
fn a_node_moves_to_where_another_node_stands() {
    let mut scene = family();
    // floor.png joins Notes, under note.png.
    assert!(move_above(&mut scene, 1, 4));
    assert_eq!(path_of(&scene, 1), Some(vec![1, 0]));
    assert_eq!(assets_of(&scene, 3), vec![1, 4, 5]);
}

#[test]
fn a_node_moves_among_the_nodes_it_already_sits_with() {
    let mut scene = family();
    // wall.png drops to where floor.png stands.
    assert!(move_above(&mut scene, 2, 1));
    assert_eq!(
        scene.root.children.iter().map(Node::id).collect::<Vec<_>>(),
        vec![2, 1, 3]
    );
}

#[test]
fn a_group_cannot_go_inside_itself() {
    let mut scene = family();
    assert!(!move_above(&mut scene, 3, 4));
    assert!(!move_above(&mut scene, 3, 3));
    // Nothing moved, and nothing was lost.
    assert_eq!(assets_of(&scene, 3), vec![4, 5]);
    assert_eq!(scene.root.children.len(), 3);
}

#[test]
fn a_move_in_order_asks_whether_the_nodes_sit_together() {
    let scene = family();
    assert_eq!(super::share_parent(&scene, &[1, 2]), Some(ROOT_ID));
    assert_eq!(super::share_parent(&scene, &[4, 5]), Some(3));
    // floor.png sits in the root, pin.png sits in Notes.
    assert_eq!(super::share_parent(&scene, &[1, 5]), None);
    assert_eq!(super::share_parent(&scene, &[]), None);
}

#[test]
fn brothers_and_sisters_move_in_order_and_keep_theirs() {
    let mut scene = family();
    // floor.png and wall.png stand under Notes. Both step up.
    assert!(super::reorder_all(&mut scene, &[1, 2], true));
    assert_eq!(
        scene.root.children.iter().map(Node::id).collect::<Vec<_>>(),
        vec![3, 1, 2]
    );
    // They cannot climb past the top.
    assert!(!super::reorder_all(&mut scene, &[1, 2], true));
}

#[test]
fn a_node_moves_into_a_group_that_holds_nothing() {
    let mut scene = family();
    scene
        .root
        .children
        .push(Node::Group(Group::new(6, "Empty".to_owned())));
    assert!(super::move_into(&mut scene, 1, 6));
    assert_eq!(assets_of(&scene, 6), vec![1]);
}

#[test]
fn a_node_moves_into_a_group_on_top_of_what_it_holds() {
    let mut scene = family();
    assert!(super::move_into(&mut scene, 1, 3));
    assert_eq!(assets_of(&scene, 3), vec![4, 5, 1]);
    assert_eq!(scene.root.children.len(), 2);
}

#[test]
fn a_group_cannot_move_into_what_it_holds() {
    let mut scene = family();
    assert!(!super::move_into(&mut scene, 3, 3));
    assert_eq!(scene.root.children.len(), 3);
}

#[test]
fn the_list_of_groups_starts_at_the_root() {
    let scene = family();
    let names: Vec<_> = super::group_names(&scene)
        .iter()
        .map(|(id, name, depth)| (*id, name.clone(), *depth))
        .collect();
    assert_eq!(
        names,
        vec![(ROOT_ID, "Scene".to_owned(), 0), (3, "Notes".to_owned(), 1)]
    );
}

#[test]
fn a_group_that_goes_leaves_its_children_where_it_stood() {
    let mut scene = family();
    assert!(super::ungroup(&mut scene, 3));
    assert_eq!(
        scene.root.children.iter().map(Node::id).collect::<Vec<_>>(),
        vec![1, 2, 4, 5]
    );
    // Every asset is still here, in the order it drew in.
    assert_eq!(
        assets(&scene)
            .iter()
            .map(|asset| asset.id)
            .collect::<Vec<_>>(),
        vec![1, 2, 4, 5]
    );
}

#[test]
fn a_group_that_goes_leaves_every_transform_alone() {
    let mut scene = family();
    // Turn and grow the group first, so its children stand apart.
    let starts = super::placed(&scene, &[3]);
    super::rotate_about(&mut scene, &starts, (1.0, 1.0), 0.7);
    let starts = super::placed(&scene, &[3]);
    super::scale_about(&mut scene, &starts, (1.0, 1.0), 1.4);
    let before = super::placed(&scene, &[3]);

    assert!(super::ungroup(&mut scene, 3));

    // An asset carries where it stands, so nothing about it changes
    // when the group around it goes.
    let after = super::placed(&scene, &[4, 5]);
    assert_eq!(before, after);
}

#[test]
fn the_root_never_goes() {
    let mut scene = family();
    assert!(!super::ungroup(&mut scene, ROOT_ID));
    // An asset is not a group, so it does not go either.
    assert!(!super::ungroup(&mut scene, 1));
    assert_eq!(scene.root.children.len(), 3);
}

#[test]
fn a_set_grows_around_a_point_and_holds_its_shape() {
    let mut scene = family();
    let starts = super::placed(&scene, &[3]);
    super::scale_about(&mut scene, &starts, (0.0, 0.0), 2.0);
    // Both assets sat at the middle, so both stay there, twice as big.
    for id in [4, 5] {
        let asset = find(&scene, id).unwrap().asset().unwrap();
        assert!((asset.scale - 2.0).abs() < 1e-9);
    }
}

#[test]
fn a_set_turns_around_a_point_and_holds_its_shape() {
    let mut scene = family();
    // pin.png stands two inches to the right of the middle.
    let Some(Node::Group(notes)) = scene.root.children.get_mut(2) else {
        panic!("Notes is the third child");
    };
    let Some(Node::Asset(pin)) = notes.children.get_mut(1) else {
        panic!("pin.png is the second child of Notes");
    };
    pin.center = (2.0, 0.0);

    let starts = super::placed(&scene, &[5]);
    super::rotate_about(&mut scene, &starts, (0.0, 0.0), std::f64::consts::FRAC_PI_2);

    let pin = find(&scene, 5).unwrap().asset().unwrap();
    // A quarter turn takes it to two inches below the middle.
    assert!(pin.center.0.abs() < 1e-9 && (pin.center.1 - 2.0).abs() < 1e-9);
    assert!((pin.rotation - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
}

#[test]
fn a_node_on_top_of_a_group_stays_where_it_is() {
    let mut scene = family();
    // pin.png is the top child of Notes, so a drop on Notes is a no-op.
    assert!(!super::move_into(&mut scene, 5, 3));
    // note.png is under it, and has somewhere to go.
    assert!(super::move_into(&mut scene, 4, 3));
    assert_eq!(assets_of(&scene, 3), vec![5, 4]);
}

#[test]
fn a_group_that_went_is_no_longer_there_to_fill() {
    let mut scene = family();
    assert!(super::has_group(&scene, 3));
    assert!(super::has_group(&scene, ROOT_ID));
    // An asset is not a group, and neither is a name nothing holds.
    assert!(!super::has_group(&scene, 1));
    super::ungroup(&mut scene, 3);
    assert!(!super::has_group(&scene, 3));
}
