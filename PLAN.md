# dmap: implementation plan

## 1. Goals and limits

- The program runs on Linux. A new build of the same code runs on Windows. The program does not use a web stack or Electron.
- One process opens two windows. The DM window is the editor. The TV window has no border and fills the TV.
- The canvas has no limits and holds many maps. The DM moves a "TV box" to select what the TV shows.
- The program uses the real size of the TV. One world unit is one real inch on the TV. One 5 ft grid cell is one inch, so miniatures fit the cells.
- The GPU calculates the light. The walls come from Foundry scenes or Universal VTT files.
- Performance is the first priority. Extension points are the second priority. Features are the third priority.

## 2. Stack decision

The stack is **Rust + wgpu + winit + egui**.

| Item | Choice | Reason |
|---|---|---|
| Language | Rust | The same code compiles for Windows. Rust prevents memory errors around the video thread and the GPU uploads. |
| GPU | wgpu | Vulkan on Linux, DX12 or Vulkan on Windows, one shader source (WGSL). Compute shaders calculate the light. |
| Windows | winit | Many windows, many monitors, full screen on a selected monitor. Works on X11 and Wayland. |
| DM UI | egui (`egui-wgpu`, `egui-winit`) | Immediate mode. Draws with the same wgpu device. New panels are easy to add. |
| Images | `image` crate (PNG, JPEG, WebP) | A worker thread decodes the file. The program uploads the image as a texture with mipmaps. |
| Vector strokes | `lyon` | Converts lines and shapes to triangles. Strokes become GPU meshes. |
| Video | `ffmpeg-next` behind a cargo feature | A thread decodes the video. The program uploads each frame as a texture. The feature is optional. |
| Files | `serde` + JSON | Foundry and UVTT files are also JSON. |

Other options and the reasons against them:
- **Bevy** gives many windows, ECS and plugins. Custom light still needs custom render graph nodes. Version changes and compile times cost too much for one person. Use Bevy only if the custom renderer becomes too large.
- **C++ with SDL2 and OpenGL or Vulkan** works. The Windows build with CMake and vcpkg costs more than it gives.

## 3. Coordinates, cameras and TV calibration

- **World space** is 2D. The camera has an f64 offset. The GPU gets f32 coordinates relative to the camera. This prevents precision loss far from the origin. The unit is the **inch**.
- **Map scale**: each map stores `grid_px`, the number of pixels in one grid cell. The world scale is `1 / grid_px`, so one cell is one inch. Foundry and UVTT files give this value. For a PNG or a JPEG, the DM clicks two grid corners or types the value.
- **TV calibration** is stored in the project file:
  - The resolution `W x H` in pixels.
  - One of these values: pixels per inch (PPI), the diagonal in inches, or the width and height in mm. The formula for the diagonal is `PPI = sqrt(W² + H²) / diagonal`.
  - At 100 % zoom the TV box is `(W / PPI, H / PPI)` inches in world space.
  - A calibration overlay shows a 1 inch grid and a 6 inch ruler on the TV. The DM compares this with a real ruler.

### 3.1 Two independent cameras

| Camera | Description | Controls |
|---|---|---|
| **DM camera** | The pan and the zoom of the editor. This is editor state only. The TV does not get it. | The controls come from Figma. The wheel pans. Shift with the wheel pans sideways. Ctrl with the wheel zooms. Ctrl with plus or minus does the same, and Ctrl with zero goes back to the start zoom. A touchpad tool such as touchegg sends a pinch as those keys, so a pinch zooms the canvas. Space with a drag pans, and so does the middle button. Home shows all. `T` shows the TV box. |
| **TV camera** | The **TV box**: a rectangle in world space with a position, a rotation and a zoom. | Drag, turn and scale the box on the DM screen. The arrow keys move the box one grid cell. |

The DM camera and the TV box are not related. The DM can zoom out to see the full canvas while the TV shows one room at 1:1. The DM can zoom in to one corner while the TV shows the full map. A "follow TV" option locks the DM camera to the TV box.

### 3.2 TV zoom and snap

- The zoom is one number on the TV box. At `zoom = 1.0` (100 %) the box is `W/PPI × H/PPI` inches. One grid cell is one real inch on the TV.
- The corner handles of the box change the zoom. A larger box shows more world, so the zoom is less than 100 %. A smaller box gives a zoom of more than 100 %. The aspect ratio is always the ratio of the TV.
- **Snap to 100 %**: the drag stops. If the zoom is in `snap_threshold` of 1.0, the zoom snaps to 1.0. A setting also permits the snap during the drag. The setting `snap_threshold` has a default of 8 %. Settings also hold an optional list of other snap levels (50 %, 200 %). Settings hold the key that stops the snap. The default key is `Alt`. The box outline changes color when the zoom is at 100 %.
- The DM can type the zoom as a number. `Ctrl` and `Alt` with the wheel change the zoom when the Table tool is active. Figma has no gesture that resizes an object with the wheel, so `Alt` marks this one as ours, and keeps the DM camera out of it. The zoom value is shown next to the box.
- A change of the box size does not change the map scale. It changes only the area that the TV shows.

## 4. Grid

The grid is a separate overlay layer (`GridLayer`). A fragment shader draws the grid in world space. The grid has no limits and costs one full-screen pass. The grid is not related to the `grid_px` of a map. A map holds its pixel scale. The grid overlay holds the look of the table.

```
GridConfig {
    kind:      Square | HexPointyTop | HexFlatTop | None,
    cell_size: f32,         // inches; square: edge length, hex: flat-to-flat width (default 1.0)
    offset:    Vec2,        // world inches
    line_width: f32,        // TV pixels, so lines stay sharp at any zoom
    color:     Rgba,
    opacity:   f32,
    style:     Solid | Dashed | Dots,   // dots = only cell corners
    show_on:   DmOnly | TvOnly | Both,
}
```

- **Look**: the line width, the color, the opacity, the style, and the screen that shows the grid. Use "DM only" when the map has a printed grid and the DM wants the snap and the cell count.
- **Size**: `cell_size` in inches. The default is 1.0, so one cell is one real inch at 100 % zoom. All values are permitted, for example 0.5 or 1.5.
- **Alignment**: the DM places each map on the canvas by hand. The program does not move the grid or the map to align them.
- **Hex**: pointy-top and flat-top. The hex math uses cube and axial coordinates in `core::grid`. The shader and the snap use the same math. Foundry hex scenes give the type (`grid.type` 2 to 5) and the size. Foundry measures a hex differently from flat-to-flat, so the importer converts the value.
- **Snap** is a `Grid` trait in `core`:
  ```rust
  trait Grid { fn snap(&self, p: Vec2) -> Vec2; fn cell_center(&self, p: Vec2) -> Vec2; fn cell_polygon(&self, p: Vec2) -> Vec<Vec2>; }
  ```
  `SquareGrid` and `HexGrid` implement the trait. The tools and the TV box use the trait. Each tool works with both grid kinds.
- Each scene has one grid. The settings hold a default grid. A later extension can give a grid to one map object. The shader then gets a short list of regions.

## 5. Architecture

The project is a cargo workspace with four crates. The dependencies go in one direction only:

```
crates/
  core/     scene model, math, commands (undo/redo), project file I/O   — no GPU, no windows
  import/   Importer trait + png/jpeg, foundry, uvtt, (video) importers  — depends on core
  render/   wgpu renderer, layer passes, light backends                  — depends on core
  app/      winit windows, egui DM UI, tools, event loop                 — depends on all
```

### 5.1 Scene model (`core`)

```
Scene
 ├─ tv: TvConfig { width_px, height_px, ppi }
 ├─ tv_box: TvBox { pos, rot, zoom }        // zoom 1.0 = physical 1:1
 ├─ grid: GridConfig                          // see §4
 └─ root: Group                 // one tree: the root holds every other node
      Group { id, name, shown: { dm, tv }, children: Vec<Node> }
      Node  = Group | Asset
      Asset { id, path, shown, transform (pos, rot, scale.xy - negative to flip), grid_px }
      // A group draws over the groups under it, and its children draw in its place.
      // A node draws for one screen only when it and every group above it show for it.
      // Walls, lights and strokes join the tree as their own kinds of node.

MapObject { asset: AssetId, transform: Transform2D (pos, rot, scale.xy — negative to flip), grid_px, opacity }
Light     { pos, bright_radius, dim_radius, color, intensity, cone_angle, rotation, animation }
Wall      { a, b, kind: Wall | Door(open/closed) | Invisible, blocks_light, blocks_sight }
```

- Each change is a `Command` with `apply` and `revert` on an undo stack. The tools make commands. The UI never changes the scene directly. This is also the extension point for scripts.
- The scene is plain data with `serde`. One scene is one folder. The folder holds `scene.json` and the images beside it. Every path in the file names a file in that folder, so the whole folder moves to another machine.
- `~/.config/dmap/config.json` holds the folder the scenes live in and the scene of the last run. It also holds the settings of the table: the TV display, swap mode and the snap window. A scene holds none of those.
- A uniform grid is the spatial index. It holds walls and objects. The program uses it for hit tests, view culling (§8.1) and the light rays. Each object keeps its world AABB. The command that moves the object updates the AABB.

### 5.2 Importers (`import`)

```rust
trait Importer {
    fn can_import(&self, path: &Path) -> bool;
    fn import(&self, path: &Path, ctx: &ImportCtx) -> Result<ImportResult>; // maps, walls, lights, grid_px
}
```

- **Image**: PNG, JPEG or WebP makes one `MapObject`. The value `grid_px` is unknown until the DM sets it.
- **Foundry VTT scene**: the importer reads a scene JSON. The JSON comes from "Export Data" on a scene, or from one line of `scenes.db`. The importer reads `background.src` or `img`, `width`, `height`, `grid.size`, `padding`, `walls[].c/door/light/sight/move`, and `lights[].x/y/config.{bright,dim,color,angle,rotation,alpha}`. Asset paths are relative to a Foundry `Data/` directory that the DM sets. The coordinates are scene pixels. The importer divides them by `grid_px`.
- **Universal VTT** (`.dd2vtt` from Dungeondraft or Arkenforge): the file holds a base64 image, `resolution.pixels_per_grid`, `line_of_sight` lines, `portals` and `lights`. This importer is small and covers most map tools.
- **Video** (feature `video`): the importer registers `mp4` and `webm`. The `MapObject` gets a `VideoSource` asset.

The program registers the importers in a list at start. A new format is one file and one `register()` line.

### 5.3 Renderer (`render`)

One `Renderer` owns the wgpu device and one surface for each window. For each frame and each window, the renderer makes a `View`. The `View` holds the camera, the viewport size and the audience (`Dm` or `Tv`). Then the renderer runs the pass list:

1. **Background pass**: clear and an optional background color.
2. **Map pass**: instanced textured quads in layer order. The textures have mipmaps and anisotropic filters, so maps do not flicker at small zoom. The program cuts textures larger than the device limit (8192 or 16384) into tiles at load time.
3. **Grid pass**: the square or hex grid shader from `GridConfig`. The pass obeys `show_on` for each audience. The grid is above the maps. The opacity keeps it soft.
4. **Drawing pass**: lyon meshes. The program keeps one mesh for each stroke and rebuilds it only when the stroke changes. The active stroke grows step by step.
5. **Light pass**: makes a light texture at the resolution of the view (see §6). The renderer multiplies it over the maps with an ambient floor. The walls are visible only in the DM view.
6. **Overlay pass** (DM only): the TV box outline, the selection handles, the gizmos, the wall and light icons.
7. **egui pass** (DM only).

The renderer requests a new frame only after an input, a scene change, an animation tick or a video frame. A scene without changes costs nothing. Animated lights and videos run a fixed tick at 60 Hz.

Extension point: `trait RenderPass { fn prepare(&mut self, scene, view); fn render(&self, encoder, view); }`. Each audience has an ordered list of passes.

### 5.4 App (`app`)

- The event loop owns two `Window` values. The TV window has no border and fills a monitor that the DM selects from a list. The DM window is a normal window.
- `Tool` trait: `on_pointer_down/move/up`, `on_key`, `draw_overlay`. Tools: Select/Transform, Pan, Pen, Line, Rect/Ellipse, Eraser, Wall, Light, TvBox, Calibrate-grid.
- egui panels: the layer list, the object properties, the TV settings, the light settings, the import dialog, the tool bar.
- Settings are for the app, not for the project. They hold the TV snap threshold, the other snap levels, the snap stop key, the default grid config and the hotkeys.
- Hotkeys:
  - Space+drag or middle drag: pan.
  - Wheel: pan. Shift with the wheel: pan sideways.
  - Ctrl with the wheel, or Ctrl with plus or minus: zoom the DM camera.
  - Ctrl with zero: back to the zoom a project opens with.
  - egui scales its own UI with these keys. The program turns that off, so a pinch zooms the map.
  - Ctrl and Alt with the wheel: zoom the TV box.
  - F: flip.
  - R: turn 90°.
  - Arrows: move the TV box one cell.
  - Ctrl with Z: undo. Ctrl and Shift with Z: redo.
  - `T`: show the TV box.
  - Tab: hide all DM overlays.

## 6. Light

Walls are segments. Lights are point or cone emitters. Two backends share one trait, so both exist at the same time:

```rust
trait LightingBackend {
    fn prepare(&mut self, walls: &[Wall], lights: &[Light], view: &View);
    fn render(&self, encoder, target: &TextureView);  // writes RGB light accumulation
}
```

**v1: hard shadows with a 1D shadow map for each light (GPU, fast)**
- For each light, the pass draws the wall segments into a 1D polar depth texture (angle to nearest distance) with 1024 to 2048 samples. Then a full-screen pass calculates the angle and the distance from each pixel to the light. The pass compares the distance with the shadow map. The light falls off from bright to dim to zero. The pass adds the colored result to the light texture. This runs 50 or more lights at 4K.
- A door toggles a wall. A door change draws the scene again.

**v2: ray traced soft shadows and bounce light**
- The program uploads the walls to a uniform grid on the GPU. For each pixel, a compute shader sends N rays to the disc of each light. The rays are stratified with temporal jitter. This gives a soft penumbra. One bounce sample gives indirect light. When the scene does not change, the shader accumulates over frames. The image converges without noise in one second.
- **2D radiance cascades** is the other option for the same slot. It gives global illumination and soft shadows from an occluder texture. It does not need ray and segment tests. Select the method after v1 is stable. Both fit the trait.

Both backends write the same light texture. The composition, the darkness slider and the DM preview stay the same.

Two additions come on top of the light. Fog of war is a texture with explored and unexplored areas. A brush or the light reach paints it. A DM-only "vision" preview shows the view from a token position.

## 7. Video maps (cargo feature)

- The `ffmpeg-next` decoder runs on its own thread. It writes frames into a ring of three RGBA buffers. NV12 with a shader conversion is an option with less bandwidth. The render thread uploads the newest frame on each tick.
- The video loops without a gap. The DM can pause and play from the UI. There is no audio.
- Windows: ffmpeg comes from vcpkg or from prebuilt shared libraries. The feature is off by default in CI, so the main build never depends on it.

## 8. Performance rules

- World coordinates: the camera offset is f64. The GPU vertices are f32 relative to the camera. The precision does not drift on the canvas.
- Textures: a worker thread decodes the file. The upload includes mipmaps. BC7 or ASTC compression is an option for later. Maps beyond the device limit become tiles. Each tile is culled and budgeted on its own. Virtual textures are for maps above 20k pixels only.
- The light is calculated once at the TV resolution. The DM view samples the same light texture when the two cameras overlap. Otherwise the DM view gets its own resolution.
- Stroke meshes are cached. The drawing layer is one buffer for each layer.
- The hot path does not allocate memory in a frame. The instance buffers are rebuilt only after a change. Dirty flags mark each layer. Only the objects that pass the culling (§8.1) go into the buffers.
- Use `tracy` (`tracing-tracy`) from the start.

### 8.1 Culling

The program culls each view against the world rectangle of that view. This is the DM camera frustum or the TV box. A rotated box uses its AABB first and then an exact test for the few border objects. The culling runs on the CPU in `render::prepare` before any GPU buffer is written. Content off the screen costs no upload and no draw call.

- **Objects**: each `MapObject`, stroke, shape, light and wall keeps a world AABB. The command that moves the object updates the AABB (dirty flag). The frame does not calculate it again. The uniform grid returns candidates for a view rectangle in O(cells touched). The exact test runs on the AABBs of the candidates. Only the objects that pass go into the instance buffers.
- **Map tiles**: large maps are already tiles at load time (§8). Each tile is culled on its own. A 16k map with one corner in the TV box uploads and draws only the tiles in that corner. Tiles far outside all views can drop their GPU texture. An LRU budget (default 2 GB) controls this. The tile comes back from the CPU copy or from disk.
- **Strokes**: each stroke has its own mesh. A stroke that is off both screens is skipped as one unit. The program cuts a very long stroke (a 500 inch path) into pieces of about 256 segments. Each piece has its own AABB.
- **Lights**: a light counts only if its dim radius disc touches the view rectangle. A wall goes to the light backend only if it touches the disc of a visible light. A wall off the screen can still make a visible shadow. The shadow map pass and the ray pass then scale with the screen content, not with the full canvas.
- **Video**: a video map that both views cull stops the decoder. The video keeps its position and continues in sync later. A video that only the DM view shows decodes at a lower rate, for example 10 fps.
- **Grid**: the grid is a full-screen shader and needs no culling. The pass is skipped when `show_on` excludes the audience.
- **One prepare for two views**: the program collects candidates once for the union rectangle. Each candidate gets a bitmask (`DM | TV`). Each window draws its own subset without a second scene walk.
- **Debug overlay** (`F3`): shows the culled and drawn counts, the resident tiles and the draw call totals for each window. A regression is visible at once.

## 9. Milestones

Each milestone ends with a program that works at the table.

**M0: skeleton (1 week)**
- The cargo workspace.
- Two winit windows with a wgpu clear color in both.
- An egui panel in the DM window.
- A monitor picker and a full-screen TV window.
- The Windows build runs once in CI (GitHub Actions matrix: ubuntu, windows).

**M1: canvas and TV box (2 weeks)**
- The DM camera with pan and zoom.
- The square grid with look settings and a canvas background color.
- PNG and JPEG import.
- The map transform tool: move, turn, scale, flip, snap.
- The TV config with the calibration overlay.
- The TV box tool with the zoom snap. The DM camera stays independent.
- The TV window shows the box.
- Object AABBs, the spatial index, and the culling for each view with the F3 debug overlay.
- Project save and load.

After M1 the program replaces a static image viewer at the table.

**M2: drawing and hex grid (1 to 2 weeks)**
- Pen, line, rect, ellipse and eraser tools with colors and widths.
- DM-only and shared layers.
- Undo and redo through the command stack.
- Hex grid variants in the shader and `HexGrid` snap.

**M3: Foundry and UVTT import (1 to 2 weeks)**
- Walls and lights are imported and drawn as DM overlays.
- `grid_px` sets the scale.
- The hex grid type and size come from Foundry scenes.
- A basic wall editor: add, delete, toggle door.
- A light placement tool.

**M4: light v1 (2 weeks)**
- The shadow map backend with light and wall culling for each view.
- The ambient and darkness controls.
- Light animations: flicker, pulse.
- Doors block or unblock the light.

**M5: light v2 (2 to 4 weeks, open)**
- Ray traced soft shadows with temporal accumulation, or radiance cascades.
- Fog of war.
- Select the method after v1 is measured.

**M6: video maps (1 to 2 weeks)**
- The ffmpeg decoder thread and the loop.
- The cargo feature.
- The Windows dependencies in the documentation.

**M7: polish**
- An asset library panel.
- A hotkey overlay.
- Many scenes in one project.
- Packaged builds: AppImage, .msi.

## 10. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Full screen on one specific monitor under Wayland | Use winit `Fullscreen::Borderless(Some(monitor))`. On X11, use a window without a border at the monitor position. Test both in M0. |
| Two swapchains with vsync cause frame stutter | Present the TV with vsync (`Fifo`). Present the DM window with `Mailbox` or `Immediate`. Render the TV first. |
| Maps above the texture limit | Cut into tiles at load time (M1), not later. |
| The Foundry format changes between versions | Read with `serde_json::Value` and a tolerant field lookup (`background.src` or `img`). Test with fixtures from v10, v11 and v12. |
| ffmpeg on Windows | Keep it behind a cargo feature. Never put it on the critical path. |
| Light v2 grows too large | Keep the trait boundary. Ship v1 first. v1 is sufficient for play. |

## 11. First steps

1. Run `cargo new --lib` for the four crates in a workspace. Pin wgpu, winit and egui versions that match each other.
2. M0: two windows that clear to a color. An egui "Hello" in the DM window. A monitor list in a combo box.
3. Write the `Scene` structs and the `Command` trait before any content is drawn. Then the M1 tools have undo from the start.
