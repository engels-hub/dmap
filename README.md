# dmap

[![aislop](https://badges.scanaislop.com/score/engels-hub/dmap.svg)](https://scanaislop.com/engels-hub/dmap)

## WARNING: This is a work in progress. Keep track of feature completion in the "Issues" section


dmap shows maps for a tabletop role-playing game on a TV.

- The program has one canvas with no limits. The canvas holds many maps.
- The game master controls the canvas in one window. A second window shows the players' view on the TV.
- The program uses the real size of the TV. One grid cell is one inch on the TV.
- The program has square grids, hex grids, tools to draw, and video maps.
- The program reads PNG, JPEG, Foundry VTT scenes and Universal VTT files.
- The GPU calculates the light. Walls and doors make shadows.
- The program runs on Linux. A new build runs on Windows. The code is Rust with wgpu, winit and egui.

## Setup

You need Rust 1.95 or later and a GPU with a Vulkan driver on Linux or a DirectX 12 driver on Windows.

1. Clone the repository.
2. Build and start the program:

```bash
cargo run --release
```

The first build takes some minutes. The DM window opens on your display. If a second display is connected, the TV window fills it. With one display, the TV window opens as a normal window.

### Where your work lives

A scene is a folder. The folder holds one `scene.json` and the map images beside it. Nothing in it points outside the folder. Copy a scene to a USB stick or to another machine, and it opens there.

```text
~/dmap/scenes/
  The Crypt/
    scene.json      the maps, where they sit, and the TV box
    crypt.png       the images themselves
  Sosnovka/
    scene.json
    village.jpg
```

The program keeps its own file in `~/.config/dmap/config.json`. It holds the folder your scenes live in and the scene you had open last. It also holds the settings of this table: the TV display, swap mode and the snap window. A scene carries none of those, so your TV does not travel with a scene you give away.

Both files hold plain JSON. Read them, and edit them by hand if you like.

Start the program with no argument and it opens the scene you had open last. Give it a scene folder to open that one:

```bash
cargo run --release -- ~/dmap/scenes/The Crypt
```

To move your scenes somewhere else, such as a campaign folder you already keep, change `scenes_dir` in the config file.

The program copies an image you add into the scene folder. The same image twice keeps one copy. A different image of the same name gets a number.

Pick the TV display in the Settings panel. The program keeps the choice in the config file.

## Controls

The canvas takes its controls from Figma.

| Action | Control |
|---|---|
| Pan the DM view | The wheel, or `Space` with a drag, or the middle button |
| Pan sideways | `Shift` with the wheel |
| Zoom the DM view | `Ctrl` with the wheel, `Ctrl` with `+` or `-`, or a pinch on a touchpad |
| Back to the zoom a project opens with | `Ctrl` with `0` |
| Put the whole TV box on the screen | `T` |

The DM view is the game master's own. The TV never moves with it.

Pick a tool in the rail on the left.

**Select** works on one map.

| Action | Control |
|---|---|
| Pick a map | Click it |
| Move it | Drag it. The corner snaps to the grid |
| Move it freely | `Ctrl` with a drag. The map keeps the spot it gets, and later steps run through that spot |
| Scale it | Drag a corner handle |
| Turn it | Drag the handle above the top edge. The angle snaps to 15 degrees |
| Turn it a quarter | `R` |
| Flip it | `F` for left to right, `Shift` with `F` for top to bottom |
| Grow or shrink it a tenth | `+` or `-` |
| Move it up or down the stack | `Page Up` or `Page Down` |
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

### Layers

Every map sits on a layer. The **Layers** list in the settings panel holds one row for each layer, the top layer first.

| Column | What it does |
|---|---|
| Name | Type a name for the layer |
| Me | Draw this layer on the DM screen |
| TV | Draw this layer on the TV |

The two switches work on their own. A layer with **Me** on and **TV** off holds what the players must not see. Keep the room they have not found there, or a note on a door. **New layer** puts another layer on top.

Pick a map with the Select tool, and the **Layer** field in the Map section moves it to another layer. A layer that is off for you takes no clicks, so a hidden map stays where you put it.

**Scenes** in the rail opens the list of your scenes. Every row is a folder.

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
