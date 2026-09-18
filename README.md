# dmap

[![aislop](https://badges.scanaislop.com/score/engels-hub/dmap.svg)](https://scanaislop.com/engels-hub/dmap)

dmap shows maps for a tabletop role-playing game on a TV, for a table with real minis.

The game master works in one window. A second window shows the players' view on the TV. One grid cell is one inch on the table, so a mini stands in one cell.

- [What dmap does](#what-dmap-does)
- [Install](#install)
- [The window](#the-window)
- [Select](#select), [Draw](#draw) and [Table](#table)
- [The Objects panel](#the-objects-panel)
- [The grid](#the-grid)
- [Undo and history](#undo-and-history)
- [Scenes and files](#scenes-and-files)
- [Settings](#settings)
- [Versions](#versions) and [Contributing](#contributing)

## What dmap does

Version 0.2.0 has these features:

- One canvas with no limits holds many maps. Drop a PNG or a JPEG on the window to add one.
- A box on the canvas decides what the TV shows. The box snaps to true size.
- Square grids and hex grids, with a tool that finds the grid on a map.
- Pens, shapes, a ruler and areas of effect for spells.
- Groups of maps and drawings, each with a switch for each screen.
- Undo, with a history that stays with the scene.
- Freeze holds the TV on one frame while you prepare the next move.
- A list of every key, and a key of your own for each control.
- A light theme, a dark theme and a translation file for each language.

dmap runs on Linux and on Windows. The code is Rust with wgpu, winit and egui.

### Where it goes next

These features are not in yet. Each one has an issue on the [Issues](https://github.com/engels-hub/dmap/issues) page.

- The resolution and the diagonal of your TV (#6)
- A 1 inch grid and a 6 inch ruler on the TV, to check the size (#7)
- Video maps (#23)
- Foundry VTT scenes and Universal VTT files (#16, #17)
- Walls, doors and light that the GPU calculates (#18, #19, #20, #21)
- Fog of war (#22)

## Install

### From a release

Download the archive for your system from the [Releases](https://github.com/engels-hub/dmap/releases) page. Unpack it and start `dmap` or `dmap.exe`. The archive holds the licenses beside the program.

You need a GPU with a Vulkan driver on Linux or a DirectX 12 driver on Windows.

### From the source

You need Rust 1.95 or later.

```bash
git clone https://github.com/engels-hub/dmap.git
cd dmap
cargo run --release
```

The first build takes some minutes.

### The first run

The DM window opens on your display. If a second display is connected, the TV window fills it. With one display, the TV window opens as a normal window.

Pick the TV display under **Settings > Table > Display**. Then add a map with **Add map**, or drop an image file on the DM window.

## The window

The toolbar at the bottom of the window comes in two boxes.

- The left box holds the views: **Select**, **Draw** and **Table**. One of them is always on.
- The right box holds **Scenes**, **Add map**, **Settings** and **Freeze**.

**History** sits in the bottom right corner. The **Objects** panel on the left shows the scene. The panel on the right belongs to the view or to what you hold.

The canvas takes its controls from Figma. These work in every view:

| Action | Control |
|---|---|
| Pan the DM view | The wheel, or `Space` with a drag, or the middle button |
| Pan sideways | `Shift` with the wheel |
| Zoom the DM view | `Ctrl` with the wheel, `Ctrl` with `+` or `-`, or a pinch on a touchpad |
| Back to the zoom a project opens with | `Ctrl` with `0` |
| Put the whole TV box on the screen | `T` |
| Take the last change back | `Ctrl` with `Z` |
| Make the change again | `Ctrl` and `Shift` with `Z`, or `Ctrl` with `Y` |
| Freeze the TV, or let it go | `P`, or the Freeze button |
| Open the list of keys | `?` |

The DM view belongs to the game master alone. The TV never moves with it.

A map the TV does not show draws faint on your own screen. You see at a glance what the players cannot see.

### Your own keys

Press `?` to open the list of keys. It is the **Shortcuts** tab of Settings. The view you work in comes first.

Press **Change** on a row, then press the new key. The program refuses a key that another control holds, and names that control. `Escape` gives up the change. **Reset to default** undoes every change.

A key that rides on a drag or a click, such as `Ctrl` for a move with no snap, stays as it is. The list shows these keys with no Change button.

### Freeze

**Freeze** holds the TV on the frame it shows at the press. Move a map, draw the next area or open another scene, and the players see none of it. Press it again to send the TV the scene as it stands.

## Select

**Select** works on the maps, the drawings and the groups on the canvas.

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
| Delete what you hold | `Delete`, or the Delete button in the panel. A group goes with everything in it |
| Set the pixels in one grid cell | Type the number in the panel, press Measure a cell and click two corners of one cell, or press Find the grid and pick two lines of the grid |
| Give up a measure | `Escape` |

A drawing takes the same three drags as a map. A growth takes the width of the line and the reach of an effect with it, so a fireball that covered four cells covers eight. One gesture is one step, whether it holds maps, drawings, or both.

Delete takes what you hold out of the scene. `Ctrl+Z` brings it all back, each node in its old place. The image files stay in the scene folder, so a delete loses nothing from the disk.

## Draw

**Draw** marks the map and lays an area of effect over it. Everything you draw sits in inches on the canvas. A drawing stays where you put it when a map moves under it.

| Action | Control |
|---|---|
| Pick a pen, a shape, the eraser or the ruler | The squares at the top of the Draw panel |
| Draw | Drag on the canvas |
| Take a bite out of a stroke | Drag the eraser over it. What is left of a shape is a free line, and the pieces join a group |
| Rub out an effect or a kept measure | Drag the eraser anywhere over it. It goes whole |
| Measure a distance | Drag the ruler. The label gives cells and feet |
| Bend the measure | The second button, while you drag |
| Keep the measure | `Shift` as you let go. Without it the line goes |
| Give up the measure | `Escape` |
| Choose how a diagonal counts | Measure, in the panel: Euclidean, D&D 5e, Pathfinder or Manhattan |
| Start a shape off the grid | Hold `Shift`, or turn "Start on the grid" off in the panel |
| Change a stroke you drew | Pick it in the Select view or in the Objects list. Its panel holds the color, the width and the size |
| Lay a burst | Drag from its middle to its edge |
| Lay a cone | Drag from its point out to where it ends. It ends as wide as it is long |
| Lay a beam | Drag for the length and let go, then drag again for the width |

A new stroke joins the Drawings group of the scene. The program keeps the color, the width, the last square you used and the way you count a diagonal. The next stroke takes them.

## Table

**Table** works on the TV box, the part of the canvas that the TV shows.

| Action | Control |
|---|---|
| Move the box | Drag it |
| Resize the box | Drag a corner handle. A size close to true size snaps to it |
| Keep a size the snap would take | Hold `Alt` as the drag ends |
| Move the box one grid cell | The arrow keys |
| Zoom the box | `Ctrl` and `Alt` with the wheel, or the Zoom field in the panel |

## The Objects panel

A scene is a tree. The root group holds everything else. Your maps, your drawings and the groups you make sit under it. The **Objects** panel shows the tree, with the top of the pile first.

The list shows one group and its contents, never the whole tree. Click a group to go into it. The path over the list names your place, such as `Scene > Group 6`. Click a part of the path to go back up.

| In a row | What it does |
|---|---|
| The name | Click it to take the row. Drag it onto a group to put it in that group, or onto an asset to take that asset's place |
| The arrow | Open or close a group. A group starts closed |
| The folder | Accent marks the group a new asset joins. A click on a group row moves the mark |
| The eye | Draw this node on the DM screen |
| The screen | Draw this node on the TV |

The two switches work on their own, and a group rules the nodes under it. A group with the screen switch off keeps every asset in it off the TV. Use it for the room the players have not found.

### Groups

- **Rename** a group: go into it, then double-click its name at the top of the list. `Enter` keeps the new name and `Escape` gives it up.
- **Move** a node into a group: drag its row onto the group. The **Group** field in the Map panel does the same.
- **Make** a group: drag over bare canvas and press **Group** in the list. The new group lands in the lowest group that held all of them.
- **Ungroup**: the contents stay where they stood, in the group that held the old group.

A group draws over the groups under it, and its assets draw inside its place. Every group but the root shows a dashed box around its contents. Click the box, or the name of the group in the list, to take the group. A group you hold has handles of its own, and everything in it keeps its place and its shape.

## The grid

One grid covers the canvas on both screens, over every map. **Settings > Grid** holds these rows:

| Row | What it sets |
|---|---|
| Canvas background | The color where no map is. **Reset** gives back the color of the theme |
| Grid line | **Automatic** or **Choose**. See below |
| Line color | The color of a chosen line |
| Line width | 0.5 to 8 points. The default is 2 |
| Line opacity | How strong the line draws |
| Kind | Square, Hex pointy top, Hex flat top or None |
| Cell size | 0.25 to 10 inches. A hex measures flat to flat |

Each color belongs to the theme you work in, so the dark theme and the light one keep grids of their own.

**Automatic**, the default, reads the map under each line and turns its light around. The line is pale over a dark map and dark over a pale one. It also keeps away from middle gray, where such a line would disappear. **Choose** draws the color you pick.

A line takes no more than a tenth of its cell, however wide you set it. The grid fades as you zoom out, and it stops at a step of 16 inches to a cell. A look at the whole map shows the map alone.

### Find the grid

A map has its own cell size in pixels. The program must know it to put one map cell on one inch.

**Find the grid**, in the panel of a map, opens the map in a dialog and marks every straight line on it. Click two lines of the grid. Two lines side by side give one cell. For a size that holds across a large map, pick two lines far apart and type the number of cells between them. A preview grid shows whether the size holds to the far edge.

**Use** writes the size to the map, and `Ctrl+Z` takes it back. The program never picks the lines for you: a map with a pattern inside each tile has lines that are not the grid. The dialog works on square grids today (#74).

## Undo and history

`Ctrl` with `Z` takes back the last change to the scene. Every change counts: a drag, a key, a field in a panel, a switch in the list, and the TV box. One drag is one step, however many frames it covers.

The stack holds a hundred steps. It lives in `history.json` beside the scene. Close the program today, and take back the same work tomorrow. Each scene keeps a stack of its own.

A field that holds the keyboard keeps an undo of its own. There `Ctrl` with `Z` works on the text you type.

**History** opens the list of your changes, the newest first. Click a step to take the scene to the state after that step. The steps you took back stay on the list in grey, so one click walks forward again. The last row, "Before the first change", takes the scene to the start of the stack.

## Scenes and files

**Scenes** in the toolbar opens the list of your scenes. Each row is a folder. The open scene shows in the accent color. The program saves a scene as you work, so a switch never asks you to save.

Caution: Delete removes the scene folder and every map in it. Nothing brings them back. The list asks you once before it does this.

| Action | Control |
|---|---|
| Put a scene on the canvas | Open |
| Make a scene and go to it | New scene |
| Give a scene another name | Rename, then type and press Enter |
| Delete a scene and its maps | Delete, then Delete again to answer the question |
| Show the folder in your file manager | Folder |
| Keep your scenes somewhere else | Change, next to the scenes folder |

Start the program with no argument to open the last scene. Give it a scene folder to open that one:

```bash
cargo run --release -- ~/dmap/scenes/The Crypt
```

### A scene folder

A scene folder holds one `scene.json`, one `history.json` and the map images. Nothing in it points outside the folder. Copy a scene to a USB stick or to another machine, and it opens there.

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

The program copies an image you add into the scene folder. The same image twice keeps one copy. A different image with the same name gets a number.

### The config file

The program keeps its own file in `~/.config/dmap/config.json`. It holds the settings of this table: the scenes folder, the last scene, the TV display, swap mode, the snap window and the keys you changed. A scene carries none of these, so your TV does not travel with a scene you give away. To keep your scenes in a campaign folder, change `scenes_dir`.

Every choice comes back on the next run: the theme, the language, the interface scale, the grid, and the Draw panel. The size and the place of the window come back too, with the view, the last Settings tab, and your place in the open scene. Another scene starts in the middle of the world.

Note: a Wayland desktop does not tell a window its place, and does not let a window set it. There the size of the window comes back, and the place does not.

These files hold plain JSON. You can read them and edit them by hand.

Caution: a change you make to `scene.json` by hand does not change `history.json`. Its steps then go back to a scene that does not exist. Delete `history.json` after such an edit. A scene with no history file opens with an empty stack.

## Settings

**Settings** in the toolbar opens a dialog with tabs. The **Table** tab holds the settings of this table and of the window:

- **Display** picks the display the TV window fills.
- **Sticks to 100 %** sets how close to true size the TV box must come before it snaps.
- **Windows (wayland compat)** sets how the TV window gets to that display. It starts on, and the two windows swap roles, because a Wayland compositor does not let a program place its own window. Turn it off, and the program moves the DM window off the TV display. X11 and Windows allow this.
- **Theme** picks the light theme, the default, or the dark one.
- **Language** holds every language the program carries. The window changes at once, with no restart. A first run takes the language of your system when a file matches it, and English when no file does.
- **Interface scale** sets the size of the toolbar, the panels and the dialogs, from 75 % to 175 % in steps of 5 %. The maps keep their size, so a change here never moves what the players see.

The **Grid** tab holds the rows in [The grid](#the-grid). The **Shortcuts** tab holds [your own keys](#your-own-keys). The **About** tab says what dmap carries and under what terms.

### Add a language

A language is one file.

1. Copy `crates/app/assets/lang/en.json`.
2. Give the copy the code of your language, such as `de.json`.
3. Translate the right-hand side of each line.
4. Send the file as a pull request. The next build carries it.

A key you leave out falls back to English, so a part of a translation also works. The log of the program names the language it took and the keys that fell back.

A value in braces, such as `{count}`, holds a number or a name. Put it where your language needs it. The line `"panel.picked.title": "{count} picked"` reads `"Выбрано: {count}"` in Russian, and both are correct.

## Versions

dmap follows semantic versioning. It is at 0.2.0 today.

- The third number goes up for a release that only fixes bugs, such as 0.1.1.
- The second number goes up for a release that adds a feature, such as 0.2.0. It resets the third to zero.
- The first number goes up when a release asks a DM to change something they already have, such as a scene file the older program cannot read.
- 1.0.0 waits until the program does everything in [Where it goes next](#where-it-goes-next).

## Contributing

dmap is free software, and it is not finished. Take a bug or a story from the [Issues](https://github.com/engels-hub/dmap/issues) page, or open one of your own.

- Read [PLAN.md](PLAN.md) for the architecture and the milestones.
- Read [DESIGN.md](DESIGN.md) before you change the window. It holds the tokens, the sizes and the shape of every screen.
- `CLAUDE.md` holds the rules for the Markdown in this repository.
- CI runs `cargo fmt --all --check`, `cargo clippy --workspace --all-targets` and `cargo test --workspace`. Run them before you push.
- Add your name to the `CONTRIBUTORS` file in the same pull request. The About tab of the program reads that file.

## License

dmap is under the GNU General Public License, version 3. See [LICENSE](LICENSE). The fonts are under the SIL Open Font License, and the icons are under the ISC license.
