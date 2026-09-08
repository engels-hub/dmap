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

The program reads and writes `project.json` in the current folder. To use another file, give its path:

```bash
cargo run --release -- path/to/project.json
```

Pick the TV display in the Settings panel. The choice is saved in the project file.

## Controls

The canvas takes its controls from Figma.

| Action | Control |
|---|---|
| Pan the DM view | The wheel, or `Space` with a drag, or the middle button |
| Pan sideways | `Shift` with the wheel |
| Zoom the DM view | `Ctrl` with the wheel, or a pinch on a touchpad |
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

Read [PLAN.md](PLAN.md) for the architecture and the milestones. Read [DESIGN.md](DESIGN.md) for the visual language of the DM window.
