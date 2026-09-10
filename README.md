# dmap

[![aislop](https://badges.scanaislop.com/score/engels-hub/dmap.svg)](https://scanaislop.com/engels-hub/dmap)

## WARNING: The list below is where dmap is going, not where it is. Keep track of feature completion in the "Issues" section


dmap shows maps for a tabletop role-playing game on a TV.

- The program has one canvas with no limits. The canvas holds many maps.
- The game master controls the canvas in one window. A second window shows the players' view on the TV.
- The program uses the real size of the TV. One grid cell is one inch on the TV.
- The program has square grids, hex grids, tools to draw, and video maps.
- The program reads PNG, JPEG, Foundry VTT scenes and Universal VTT files.
- The GPU calculates the light. Walls and doors make shadows.
- The program runs on Linux. A new build runs on Windows. The code is Rust with wgpu, winit and egui.

## Versions

dmap follows semantic versioning. It is at 0.1.1 today.

- The third number goes up for a release that only fixes bugs, such as 0.1.1.
- The second number goes up for a release that adds a feature, such as 0.2.0. It resets the third to zero.
- The first number goes up when a release asks a DM to change something they already have, such as a scene file the older program cannot read.
- 1.0.0 waits until this page describes only what the program does.

## Contributing

dmap is free software, and it is not finished. Take a bug or a story from the [Issues](https://github.com/engels-hub/dmap/issues) page, or open one of your own.

- Read `DESIGN.md` before you touch the window. It holds the tokens, the sizes and the shape of every screen.
- `CLAUDE.md` holds the rules the Markdown in this repository follows.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets` and `cargo test --workspace` all run in CI. Run them before you push.
- Add your name to the `CONTRIBUTORS` file in the same pull request. The About tab of the program reads that file.

## Setup

You need Rust 1.95 or later and a GPU with a Vulkan driver on Linux or a DirectX 12 driver on Windows.

1. Clone the repository.
2. Build and start the program:

```bash
cargo run --release
```

The first build takes some minutes. The DM window opens on your display. If a second display is connected, the TV window fills it. With one display, the TV window opens as a normal window.

### Where your work lives

A scene is a folder. The folder holds one `scene.json`, one `history.json` and the map images beside them. Nothing in it points outside the folder. Copy a scene to a USB stick or to another machine, and it opens there.

```text
~/dmap/scenes/
  The Crypt/
    scene.json      the maps, where they sit, and the TV box
    history.json    the changes you can take back
    crypt.png       the images themselves
  Sosnovka/
    scene.json
    history.json
    village.jpg
```

The program keeps its own file in `~/.config/dmap/config.json`. It holds the folder your scenes live in and the scene you had open last. It also holds the settings of this table: the TV display, swap mode and the snap window. A scene carries none of those, so your TV does not travel with a scene you give away.

These files hold plain JSON. Read them, and edit them by hand if you like.

Caution: a change you make to `scene.json` by hand leaves `history.json` behind. The steps in it go back to a scene that no longer stands there. Delete `history.json` after such an edit. A scene with no history file opens with an empty stack.

Start the program with no argument and it opens the scene you had open last. Give it a scene folder to open that one:

```bash
cargo run --release -- ~/dmap/scenes/The Crypt
```

To move your scenes somewhere else, such as a campaign folder you already keep, change `scenes_dir` in the config file.

The program copies an image you add into the scene folder. The same image twice keeps one copy. A different image of the same name gets a number.

Pick the TV display in the Settings dialog. The program keeps the choice in the config file.

The window has a light theme and a dark theme. The light one is the default. Pick the other one under **Theme** in the Settings dialog, and the program keeps that choice too.

**Language** in the same dialog holds every language the program carries. The window takes a new one at once, with no restart, and the program keeps the choice. A first run takes the language of your system when a file matches it, and English when none does.

To add a language, write one file. Copy `crates/app/assets/lang/en.json`, name the copy for your language, such as `ru.json`, and translate the right-hand side of each line. A key you leave out falls back to English, so a part of a translation is a translation. The program says in its log which language it took and which keys fell back. Send the file as a pull request, and the next build carries it.

A place for a number or a name stands in braces, such as `{count}`. Put it where your language wants it. The line `"panel.picked.title": "{count} picked"` reads `"Выбрано: {count}"` in Russian, and both are right.

**Interface scale** in the same dialog sets how big the toolbar, the panels and the dialogs draw. The range is 75 % to 175 %, in steps of 5 %. The maps keep their size, and one grid cell stays one inch, so a change here never moves what the players see.

## Controls

The canvas takes its controls from Figma.

| Action | Control |
|---|---|
| Pan the DM view | The wheel, or `Space` with a drag, or the middle button |
| Pan sideways | `Shift` with the wheel |
| Zoom the DM view | `Ctrl` with the wheel, `Ctrl` with `+` or `-`, or a pinch on a touchpad |
| Back to the zoom a project opens with | `Ctrl` with `0` |
| Put the whole TV box on the screen | `T` |
| Take the last change back | `Ctrl` with `Z` |
| Make the change again | `Ctrl` and `Shift` with `Z`, or `Ctrl` with `Y` |

The DM view is the game master's own. The TV never moves with it.

`Ctrl` with `Z` takes back the last change to the scene. Every change counts: a drag, a key, a field in a panel, a switch in the list, and the TV box. One drag is one step, however many frames it covers. The stack holds a hundred steps. It lives in `history.json` beside the scene, so you close the program and take back yesterday's work tomorrow. Another scene on the canvas brings the stack of its own folder.

A field that holds the keyboard keeps an undo of its own. There `Ctrl` with `Z` works on the text you type.

The **History** button in the bottom right corner opens the list of your changes, the newest first. A click on a step takes the scene to the state after that step. The steps you took back stay on the list, in grey, so one click walks forward again. The last row, "Before the first change", takes the scene to where the stack begins.

Pick a view in the toolbar at the bottom of the window. The toolbar comes in two boxes. The left one holds the views, and one of them is always on. The right one holds Scenes, Add map and Settings, which open something at once and never stay on.

**Select** works on the assets and the groups on the canvas.

| Action | Control |
|---|---|
| Pick an asset | Click it |
| Pick a group | Click its dashed box |
| Add to what you hold | `Ctrl` with a click |
| Pick everything in a patch | Drag over bare canvas. A list says what you took |
| Group what you hold | Group, in that list |
| Move it | Drag it. The corner snaps to the grid. A group takes its assets along |
| Move it freely | `Ctrl` with a drag. The map keeps the spot it gets, and later steps run through that spot |
| Scale it | Drag a corner handle |
| Turn it | Drag the handle above the top edge. The angle snaps to 15 degrees |
| Turn it a quarter | `R` |
| Flip it | `F` for left to right, `Shift` with `F` for top to bottom |
| Grow or shrink it a tenth | `+` or `-` |
| Move it up or down the stack | `Page Up` or `Page Down`. Nodes that sit in one group move together |
| Set the pixels in one grid cell | Type the number in the panel, or press Measure a cell and click two corners of one cell |
| Give up a measure | `Escape` |

**Table** works on the TV box, the part of the canvas the TV shows.

| Action | Control |
|---|---|
| Move the box | Drag it |
| Resize the box | Drag a corner handle. A size close to true size snaps to it |
| Keep a size the snap would take | Hold `Alt` as the drag ends |
| Move the box one grid cell | The arrow keys |
| Zoom the box | `Ctrl` and `Alt` with the wheel, or the Zoom field in the panel |

Add a map with the Add map button, or drop an image file on the DM window.

### Groups and assets

A scene is a tree. The root group holds everything else, and under it sit your assets and the groups you make. The **Objects** panel on the left shows that tree, with the top of the pile first.

The list shows one group and what is in it, never the whole tree. Click a group and the list goes into it. The path over the list names where you are, such as `Scene > Group 6`, and a click on a part of the path takes you back up.

| In a row | What it does |
|---|---|
| The name | Click it to take the row. Drag it onto a group to put it in that group, or onto an asset to take that asset's place |
| The arrow | Open or close a group. A group starts closed |
| The folder | Accent marks the group a new asset joins. A click on a group row moves the mark |
| The eye | Draw this node on the DM screen |
| The screen | Draw this node on the TV |

To rename a group, go into it with a click, then double-click its name at the top of the list. `Enter` keeps the new name and `Escape` gives it up.

The two switches work on their own, and a group rules the nodes under it. A group with the screen switch off keeps every asset in it off the TV, whatever the asset says. Use it for the room the players have not found.

A map the TV does not show draws faint on your own screen. So you see at a glance what the players cannot, without a look at the list.

One grid covers the canvas on both screens, over every map. One cell is one inch on the table.

To move something you already placed, drag its row onto a group. The **Group** field in the Map panel does the same without a drag.

A group draws over the groups under it, and its assets draw inside its place. Every group but the root shows a dashed box around what it holds. Click the box, or the group's name in the list, to take the group.

A group you hold takes handles of its own. Drag the box to move it, a corner to grow it, and the handle above it to turn it. Everything in the group keeps its place and its shape.

**Ungroup** takes a group apart. What was in it stays where it stood, in the group that held it.

To take several things, drag over bare canvas. A list says what you took, and **Group** puts them in a new group. The new group lands in the lowest group that held them all. An uncle and a nephew meet in the group above both.

**Scenes** in the toolbar opens the list of your scenes. Every row is a folder.

Caution: Delete takes the scene folder and every map in it. Nothing brings them back. The list asks you once before it does this.

| Action | Control |
|---|---|
| Put a scene on the canvas | Open |
| Make a scene and go to it | New scene |
| Give a scene another name | Rename, then type and press Enter |
| Delete a scene and its maps | Delete, then Delete again to answer the question |
| Show the folder in your file manager | Folder |
| Keep your scenes somewhere else | Change, next to the scenes folder |

The open scene stands out in the accent color. The program saves a scene as you work, so a switch never asks you to save.

Read [PLAN.md](PLAN.md) for the architecture and the milestones. Read [DESIGN.md](DESIGN.md) for the visual language of the DM window.
