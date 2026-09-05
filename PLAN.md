# TableMap — DnD TV map display: implementation plan

## 1. Goals and constraints

- Linux native first, Windows by recompile. No web stack, no Electron.
- Two windows from one process: **DM window** (editor UI) and **TV window** (borderless fullscreen on the table display).
- Infinite canvas holding many maps; DM chooses what the TV shows by moving a "TV box".
- Physical calibration: 1 world unit = 1 real inch on the TV, so a 5 ft grid square is exactly 1 inch and minis fit.
- Lighting simulated on the GPU with walls from Foundry / Universal VTT scenes.
- Performance first, extension points second, features third.

## 2. Stack decision

**Rust + wgpu + winit + egui.**

| Concern | Choice | Why |
|---|---|---|
| Language | Rust | Cross-compiles to Windows with no code changes; memory safety around a threaded video decoder and GPU uploads. |
| GPU | wgpu | Vulkan on Linux, DX12/Vulkan on Windows, same shader source (WGSL). Compute shaders for lighting. |
| Windowing | winit | Multi-window, multi-monitor, fullscreen on a chosen monitor, works on X11 and Wayland. |
| DM UI | egui (`egui-wgpu`, `egui-winit`) | Immediate-mode, renders into the same wgpu device, trivial to add panels as features grow. |
| Images | `image` crate (PNG/JPEG/WebP) | Decode on a worker thread, upload as mipmapped textures. |
| Vector strokes | `lyon` | Tessellates polylines/shapes to triangles; strokes become GPU meshes. |
| Video | `ffmpeg-next` behind a cargo feature | Decode on a thread, upload frames as textures. Optional, so a build without ffmpeg still works. |
| Serialization | `serde` + JSON (project file) | Foundry and UVTT are JSON anyway. |

Rejected alternatives, briefly:
- **Bevy**: gives multi-window, ECS and plugins for free, but custom lighting still means custom render graph nodes, and version churn plus compile times are a tax on a one-person project. Revisit only if the custom renderer becomes a burden.
- **C++ + SDL2 + OpenGL/Vulkan**: fully viable, but the Windows build story (deps, CMake, vcpkg) costs more than it returns here.

## 3. Coordinate model, cameras and calibration

- **World space**: 2D, f64 origin offset + f32 local coordinates on the GPU to avoid precision loss far from origin. Unit = **inches**.
- **Map scale**: each map stores `grid_px` (pixels per grid square). World scale = `1 / grid_px` so one square = 1 inch. Foundry and UVTT provide this; for plain PNG/JPEG the DM sets it with a "click two grid corners" tool or types it.
- **TV calibration** (stored in project):
  - Resolution `W x H` in pixels.
  - One of: pixels per inch (PPI), or diagonal in inches → `PPI = sqrt(W² + H²) / diagonal`, or physical width/height in mm.
  - TV box world size at 100 % zoom = `(W / PPI, H / PPI)` inches.
  - A calibration overlay draws a 1-inch grid and a 6-inch ruler on the TV so the DM can check with a real ruler.

### 3.1 Two independent cameras

| Camera | What it is | Controlled by |
|---|---|---|
| **DM camera** | Pan/zoom of the editor viewport. Pure editor state, never saved to the TV. | Scroll to zoom, space+drag / middle-drag to pan, Home to frame everything, `T` to frame the TV box. |
| **TV camera** | The **TV box**: a rectangle in world space with position, rotation and zoom. | Dragging, rotating and scaling the box on the DM screen. Arrow keys nudge by one grid cell. |

The DM camera and the TV box are unrelated: the DM can zoom out to see the whole canvas while the TV keeps showing a 1:1 room, or zoom into a corner while the TV shows the wide map. Optional "follow TV" toggle locks the DM camera to the box for the DM who prefers to see what the players see.

### 3.2 TV zoom and snapping

- Zoom is a scalar on the TV box. `zoom = 1.0` (100 %) means the box is exactly `W/PPI × H/PPI` inches and a grid cell is one physical inch on the TV.
- Scaling the box with its corner handles changes zoom. Bigger box → more world on screen → zoom below 100 %; smaller box → zoom above 100 %. Aspect ratio always locked to the TV.
- **Snap to 100 %**: when a handle drag ends (or during the drag, configurable) with zoom within `snap_threshold` of 1.0, it snaps to exactly 1.0. `snap_threshold` lives in settings (default 8 %) together with an optional list of extra snap levels (50 %, 200 %) and the modifier key that suppresses snapping (`Alt` by default). The box outline changes color while snapped so the DM can see 1:1 is active.
- Zoom is also editable as a number and via `Ctrl`+scroll while the TvBox tool is active. The zoom value is shown next to the box.
- Scaling the box never changes map scale; it only changes how much world the TV shows.

## 4. Grid

The grid is its own overlay layer (`GridLayer`), drawn procedurally in a fragment shader in world space, so it is infinite and costs one fullscreen pass regardless of size. It is independent of any map's `grid_px`; maps carry their pixel scale, the grid overlay carries how the table looks.

```
GridConfig {
    kind:      Square | HexPointyTop | HexFlatTop | None,
    cell_size: f32,         // inches; square: edge length, hex: flat-to-flat width (default 1.0)
    offset:    Vec2,        // world inches, to align with a map's own printed grid
    line_width: f32,        // in TV pixels, so it stays crisp at any zoom
    color:     Rgba,
    opacity:   f32,
    style:     Solid | Dashed | Dots,   // dots = only cell corners
    show_on:   DmOnly | TvOnly | Both,
}
```

- **Appearance**: line width, color, opacity, style, and which screen shows it. DM-only is handy when the map already has a printed grid but the DM wants snapping and cell counting.
- **Size**: `cell_size` in inches. Default 1.0 so one cell is one physical inch at 100 % zoom. Any value is allowed (e.g. 0.5 for gridless-feel or 1.5 for large minis).
- **Alignment**: an "align to map" action sets `offset` and `cell_size` from the selected map's `grid_px` and transform, so the overlay lines fall on the map's printed lines. Manual offset drag as fallback.
- **Hex** (nice to have, but cheap because it is only shader math): pointy-top and flat-top variants. Hex math in cube/axial coordinates lives in `core::grid` and is used for both the shader's distance-to-edge function and for snapping. Foundry hex scenes import their type (`grid.type` 2–5 → row/column hexes) and size; Foundry measures hex size differently from flat-to-flat, so the importer converts.
- **Snapping** is a `Grid` trait in `core`:
  ```rust
  trait Grid { fn snap(&self, p: Vec2) -> Vec2; fn cell_center(&self, p: Vec2) -> Vec2; fn cell_polygon(&self, p: Vec2) -> Vec<Vec2>; }
  ```
  Implemented by `SquareGrid` and `HexGrid`; tools and the TV box nudge use it, so every tool works with both grid kinds without knowing which.
- Per-scene grid, with a global default in settings. A later extension is per-region grids (e.g. one hex map next to a square map) by attaching a `GridConfig` to a map object; the shader takes a small list of regions, so the design allows it.

## 5. Architecture

Cargo workspace, four crates, dependencies only flowing downward:

```
crates/
  core/     scene model, math, commands (undo/redo), project file I/O   — no GPU, no windowing
  import/   Importer trait + png/jpeg, foundry, uvtt, (video) importers  — depends on core
  render/   wgpu renderer, layer passes, lighting backends              — depends on core
  app/      winit windows, egui DM UI, tools, event loop                 — depends on all
```

### 5.1 Scene model (`core`)

```
Scene
 ├─ tv: TvConfig { width_px, height_px, ppi }
 ├─ tv_box: TvBox { pos, rot, zoom }        // zoom 1.0 = physical 1:1
 ├─ grid: GridConfig                          // see §4
 ├─ layers: Vec<Layer>          // ordered, each with visibility: DmOnly | Both
 │    ├─ MapLayer   { maps: Vec<MapObject> }
 │    ├─ DrawLayer  { strokes: Vec<Stroke>, shapes: Vec<Shape> }
 │    ├─ WallLayer  { walls: Vec<Wall> }        // segments, door flag, blocks_light/blocks_sight
 │    └─ LightLayer { lights: Vec<Light>, ambient: Color, darkness: f32 }
 └─ assets: AssetStore          // id -> path, hash; resolves relative to project dir

MapObject { asset: AssetId, transform: Transform2D (pos, rot, scale.xy — negative for flip), grid_px, opacity }
Light     { pos, bright_radius, dim_radius, color, intensity, cone_angle, rotation, animation }
Wall      { a, b, kind: Wall | Door(open/closed) | Invisible, blocks_light, blocks_sight }
```

- Every mutation is a `Command` (`apply`, `revert`) on an undo stack. Tools emit commands; UI never mutates the scene directly. This is also the extension seam for scripting later.
- Scene is plain data with `serde`. Project file = `project.json` + `assets/` directory, relative paths.
- Spatial index: a uniform grid over walls and objects for hit testing, view culling (§8.1) and the lighting raycasts. Every object caches its world AABB, refreshed by the command that moves it.

### 5.2 Importers (`import`)

```rust
trait Importer {
    fn can_import(&self, path: &Path) -> bool;
    fn import(&self, path: &Path, ctx: &ImportCtx) -> Result<ImportResult>; // maps, walls, lights, grid_px
}
```

- **Image**: PNG/JPEG/WebP → one `MapObject`, grid_px unknown until set.
- **Foundry VTT scene**: accepts a scene JSON (from "Export Data" on a scene, or a line from `scenes.db`). Reads `background.src` / `img`, `width`, `height`, `grid.size`, `padding`, `walls[].c/door/light/sight/move`, `lights[].x/y/config.{bright,dim,color,angle,rotation,alpha}`. Asset paths resolve against a user-configured Foundry `Data/` directory. Coordinates are in scene pixels, converted by `grid_px`.
- **Universal VTT** (`.dd2vtt`, Dungeondraft/Arkenforge): base64 image, `resolution.pixels_per_grid`, `line_of_sight` polylines, `portals`, `lights`. Cheap to add next to Foundry and covers most map-maker exports.
- **Video** (feature `video`): registers an `mp4/webm` importer producing a `MapObject` whose asset is a `VideoSource`.

Importers are registered in a list at startup, so a new format is one file and one `register()` line.

### 5.3 Renderer (`render`)

One `Renderer` owns the wgpu device and per-window surfaces. Each frame, for each window, it builds a `View` (camera, viewport size, audience = Dm | Tv) and runs the pass list:

1. **Background pass**: clear + optional backdrop color.
2. **Map pass**: textured quads, instanced, sorted by layer order. Mipmapped, anisotropic sampling so zoomed-out maps do not shimmer. Textures larger than the device limit (8192 or 16384) are split into tiles at load.
3. **Grid pass**: procedural square/hex grid shader from `GridConfig`, honoring `show_on` per audience. Drawn above maps so it stays visible; opacity keeps it unobtrusive.
3b. **Drawing pass**: lyon-tessellated meshes, cached per stroke, rebuilt only when the stroke changes. Active stroke drawn incrementally.
4. **Lighting pass**: produces a light texture at the view's resolution (see §6), composited multiplicatively over maps with ambient floor. Walls rendered for the DM view only.
5. **Overlay pass** (DM only): TV box outline, selection handles, gizmos, wall/light icons.
6. **egui pass** (DM only).

Redraw policy: request a redraw only on input, scene change, animation tick or video frame. Idle scenes cost nothing. Animated lights and videos drive a fixed 60 Hz tick.

Extension seam: `trait RenderPass { fn prepare(&mut self, scene, view); fn render(&self, encoder, view); }` with an ordered list per audience.

### 5.4 App (`app`)

- Event loop owns two `Window`s. TV window: borderless fullscreen on a monitor the DM picks from a list; DM window: normal.
- `Tool` trait: `on_pointer_down/move/up`, `on_key`, `draw_overlay`. Tools: Select/Transform, Pan, Pen, Line, Rect/Ellipse, Eraser, Wall, Light, TvBox, Calibrate-grid.
- egui panels: layer list, object properties, TV settings, lighting settings, import dialog, tool bar.
- Settings (app-level, not per project): TV snap threshold and extra snap levels, snap-suppress modifier, default grid config, hotkeys.
- Hotkeys: space+drag pan, scroll zoom (DM camera only), F to flip, R rotate 90°, arrows nudge TV box by one cell, `T` frame TV box, Tab hide all DM overlays.

## 6. Lighting

Walls are segments; lights are point/cone emitters. Two backends behind one trait so they can coexist and be compared:

```rust
trait LightingBackend {
    fn prepare(&mut self, walls: &[Wall], lights: &[Light], view: &View);
    fn render(&self, encoder, target: &TextureView);  // writes RGB light accumulation
}
```

**v1 — Hard shadows, per-light 1D shadow map (GPU, fast)**
- For each light: render wall segments into a 1D polar depth texture (angle → nearest distance), 1024–2048 samples. Then a fullscreen pass computes, per pixel, angle and distance to the light and compares against the shadow map for occlusion. Distance falloff bright→dim→0, colored, additive blend into the light texture. 50+ lights at 4K is fine.
- Doors toggle walls; door state changes just re-render.

**v2 — Ray traced soft shadows and bounce (the "ray/path tracing" item)**
- Walls uploaded to a GPU uniform grid. A compute shader, per pixel, shoots N rays (stratified, temporal jitter) toward each light's disc radius → soft penumbra, and optionally one bounce sample for indirect light. Temporal accumulation over frames when the scene is static (which it usually is at the table) gives a converged, noise-free result within a second.
- Alternative for the same slot: **2D radiance cascades**, which give GI and soft shadows from an occluder texture without ray/segment tests. Pick after v1 is stable; the trait boundary means either drops in.

Both write the same light texture, so composition, darkness slider, and DM preview do not change between backends.

Extras on top of lighting: fog of war (explored/unexplored texture painted by revealing brush or by light reach), and DM-only "vision" preview from a token position.

## 7. Video maps (feature-gated)

- `ffmpeg-next` decoder on a dedicated thread, outputs frames into a triple-buffered ring of RGBA (or NV12 + shader conversion for less bandwidth). Render thread uploads the newest frame per tick.
- Loop seamlessly, pause/play from DM UI, no audio.
- Windows: ffmpeg via vcpkg or prebuilt shared libs; the feature is off by default in CI so the main build never depends on it.

## 8. Performance rules

- World coordinates: f64 camera offset, f32 GPU vertices relative to the camera. No precision drift on an "infinite" canvas.
- Textures: decode off-thread, upload with mipmaps, BC7/ASTC compression optional later. Tiles for maps beyond device limits, culled and budgeted individually; virtual texturing only if 20k+ px maps show up.
- Lighting computed at TV resolution once; DM view samples the same light texture when its camera overlaps, otherwise at DM resolution.
- Stroke meshes cached; drawing layer batched into one buffer per layer.
- Nothing allocates per frame in the hot path; scene → GPU instance buffers rebuilt only on change (dirty flags per layer) and only for objects that survive culling (§8.1).
- Profile with `tracy` (`tracing-tracy`) from the start.

### 8.1 Culling

Everything is culled per view against that view's world-space rectangle (the DM camera's frustum or the TV box, rotated box → use its AABB, then exact test for the few borderline objects). Culling happens on the CPU in `render::prepare`, before any GPU buffer is written, so offscreen content costs neither upload nor draw calls.

- **Objects**: each `MapObject`, stroke, shape, light and wall keeps a cached world AABB, updated by the command that moves it (dirty flag), never recomputed per frame. The scene's uniform-grid spatial index returns candidates for a view rect in O(cells touched); the candidates' AABBs are tested exactly. Only survivors go into the instance buffers.
- **Map tiles**: large maps are already split into tiles at load (§8). Tiles are culled individually, so a 16k map with only one corner in the TV box uploads and draws only that corner's tiles. Tiles far outside any view can also drop their GPU texture (LRU budget, default 2 GB) and reload from the decoded CPU copy or disk when they come back.
- **Strokes**: cached meshes are per stroke, so a stroke off both screens is skipped whole. Very long strokes (a 500-inch path) are split into chunks of ~256 segments with their own AABBs.
- **Lights**: a light contributes only if its dim-radius disc intersects the view rect. Walls feed the lighting backend only if they intersect the union of visible lights' discs (they can occlude a visible light while being offscreen themselves), which keeps the shadow-map and ray passes proportional to what is on screen rather than to the whole campaign canvas.
- **Video**: a video map that is culled from *both* views pauses decoding (keeps its position so it resumes in sync). Decoding for a video visible only on the DM screen continues at reduced rate (e.g. 10 fps) since it is a preview.
- **Grid** is a fullscreen shader and needs no culling; it is skipped entirely when `show_on` excludes the audience.
- **Two views share one prepare**: candidates are gathered once for the union rect, then tagged with a bitmask (`DM | TV`) so each window's draw uses its own subset without a second scene traversal.
- **Debug overlay** (`F3`): shows culled/drawn counts, tile residency and draw-call totals per window, so regressions are visible immediately.

## 9. Milestones

Each milestone ends in something usable at the table.

**M0 — Skeleton (1 week)**
Workspace, two winit windows, wgpu clear color in both, egui panel in DM window, monitor picker, fullscreen TV. Windows build verified once in CI (GitHub Actions matrix: ubuntu, windows).

**M1 — Canvas and TV box (2 weeks)**
DM camera (pan/zoom), infinite square grid with appearance settings, PNG/JPEG import, map transform tool (move, rotate, scale, flip, snap), TV config + calibration overlay, TV box tool with zoom snap and independent DM camera, TV window renders the box. Grid align-to-map. Object AABBs + spatial index + per-view culling with the F3 debug overlay. Project save/load.
→ Already replaces a static image viewer at the table.

**M2 — Drawing and hex grid (1–2 weeks)**
Pen/line/rect/ellipse/eraser, colors and widths, DM-only vs shared layers, undo/redo through the command stack. Hex grid variants in shader and `HexGrid` snapping.

**M3 — Foundry and UVTT import (1–2 weeks)**
Walls and lights imported and drawn as DM overlays, `grid_px` auto scale, hex grid type/size from Foundry scenes, basic wall editor (add, delete, toggle door), light placement tool.

**M4 — Lighting v1 (2 weeks)**
Shadow-map backend with light/wall culling per view, ambient/darkness controls, light animations (flicker, pulse), doors block/unblock light.

**M5 — Lighting v2 (2–4 weeks, open-ended)**
Ray traced soft shadows with temporal accumulation, or radiance cascades. Fog of war. Choose after measuring v1.

**M6 — Video maps (1–2 weeks)**
ffmpeg decode thread, looping, feature flag, Windows deps documented.

**M7 — Polish**
Asset library panel, hotkey overlay, multiple scenes per project, packaged builds (AppImage / .msi).

## 10. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Fullscreen on a specific monitor under Wayland | winit `Fullscreen::Borderless(Some(monitor))`; fall back to a borderless window positioned by monitor geometry on X11. Test both early in M0. |
| Two swapchains, one vsync each → frame pacing hitches | Present TV with vsync (`Fifo`), DM window with `Mailbox`/`Immediate`; render TV first. |
| Maps above texture limits | Tile at load (M1), not later. |
| Foundry format drifts between versions | Importer reads with `serde_json::Value` and tolerant field lookup (`background.src` or `img`), tests with fixtures from v10, v11, v12. |
| ffmpeg on Windows | Feature-gated; never on the critical path. |
| Lighting v2 scope creep | Trait boundary; v1 ships first and is good enough for play. |

## 11. First steps

1. `cargo new --lib` the four crates in a workspace; pin wgpu/winit/egui versions that are known to match each other.
2. M0: two windows, both clearing to a color, egui "Hello" in the DM window, monitor list in a combo box.
3. Write the `Scene` structs and the `Command` trait before any rendering of content, so M1 tools are built on undo from the start.
