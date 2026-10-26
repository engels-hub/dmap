//! The light pass: darkness, the light of each lamp, and the shadows of
//! the walls. PLAN.md 6, v1. Issues #19 and #20.
//!
//! The pass runs in three steps. The first step draws a shadow map: one
//! row for each light, one column for each of [`SAMPLES`] directions
//! around it, and in each texel the distance to the nearest wall that way.
//! The second step adds up the light of each lamp that reaches a point,
//! unless the shadow map says a wall stands between the two. Each lamp
//! draws over the box of its reach only. The step runs at one
//! [`LAMP_SCALE`]th of the screen size, because the light changes slowly
//! across the screen. The third step runs over the whole screen.
//! Each pixel takes the ambient light and the lamp light near it. The sum
//! multiplies the maps under it.
//!
//! A stroke that reveals draws onto a mask at the full screen size, so
//! its edge stays as sharp as the ink. The map under the mask shows as if
//! a lamp lit it. Each part of the stroke sees from an eye, and the eyes
//! take rows of the shadow map as the lights do, so the walls that stop
//! sight stop the reveal. Issue #65.
//!
//! The light draws over the maps and under the grid and the strokes, so a
//! line the DM drew still reads in the dark. A scene that holds no light
//! and no stroke that reveals shows its maps as they are, so a plain map
//! never goes dark.

// Rust guideline compliant 2026-02-21

use egui_wgpu::wgpu;
use serde::{Deserialize, Serialize};

use crate::camera::Camera;
use crate::ink::{CONCEAL, Reveal};
use crate::scene::{Audience, Node, Scene};
use crate::sight::{Darkness, Fill, Light, Wall};

/// How many directions the shadow map holds for each light.
///
/// A wall at the edge of a light 20 cells across then moves by less than
/// a tenth of a cell between two samples. PLAN.md 6 asks for 1024 to 2048.
pub const SAMPLES: u32 = 1024;

/// How many screen pixels one texel of the lamp light covers, each way.
///
/// One texel for 4 x 4 pixels takes a sixteenth of the work of the full
/// screen. A shadow edge then blurs over 4 pixels. The `SCALE` of the light shader
/// holds the same number.
const LAMP_SCALE: u32 = 4;

/// Which screens draw the light. DESIGN.md 9.3.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Show {
    /// The DM screen and the TV.
    #[default]
    Both,
    /// The DM screen alone, so the DM can prepare the light first.
    Dm,
    /// The TV alone, so the DM still sees the whole map.
    Tv,
}

impl Show {
    /// Whether this choice draws the light on `audience`.
    pub fn on(self, audience: Audience) -> bool {
        matches!(
            (self, audience),
            (Self::Both, _) | (Self::Dm, Audience::Dm) | (Self::Tv, Audience::Tv)
        )
    }
}

/// How dark a new install starts, when the scene holds a light.
///
/// A scene with no light shows its maps as they are, whatever this says,
/// so a plain map never goes dark. A Foundry or a Universal VTT import
/// brings lights, and the DM then sees them at once.
pub const DEFAULT_DARKNESS: f64 = 0.75;

/// How much of the darkness the DM sees through on a new install.
///
/// The DM must see what lies under the fog. The blue hatch of the light
/// shader tells it from what the players see, so most of the darkness
/// can go. Issue #65.
pub const DM_SEE_THROUGH: f64 = 0.7;

/// How dark a scene is, and the color of the light its darkness leaves.
///
/// The scene file holds this, so a sunlit yard and a crypt each keep
/// their own. An area of darkness holds its own darkness on top of it.
/// Issue #20.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mood {
    /// How dark the parts no lamp reaches are: 0 is the map as it is, 1
    /// is black.
    pub darkness: f64,
    /// The color of the light that the darkness leaves.
    pub ambient: [u8; 3],
}

impl Default for Mood {
    fn default() -> Self {
        Self {
            darkness: DEFAULT_DARKNESS,
            ambient: [255, 255, 255],
        }
    }
}

/// How walls and doors treat the light, at this table. DESIGN.md 9.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rules {
    /// Whether walls cast shadows.
    pub walls_block: bool,
    /// Whether an open door lets the light through.
    pub open_doors_pass: bool,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            walls_block: true,
            open_doors_pass: true,
        }
    }
}

/// One light as the shader reads it. The layout follows the WGSL struct.
///
/// Center x and y, bright reach, dim reach, then red, green, blue and a
/// pad, then the cone, the turn, and where the walls of this light start
/// in the wall list and how many there are. 48 bytes, on the 16 byte
/// boundary a storage array wants.
type GpuLight = [f32; 12];

/// One wall as the shader reads it: both ends.
type GpuWall = [f32; 4];

/// One area of darkness as the shader reads it: its four corners, its
/// darkness, the code of its fill, and two pads. 48 bytes.
type GpuZone = [f32; 12];

/// What one view of the light needs, packed for the GPU.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Input {
    lights: Vec<GpuLight>,
    walls: Vec<GpuWall>,
    zones: Vec<GpuZone>,
    /// Whether an area shows a fill other than black, so the fill pass
    /// has something to draw.
    fills: bool,
    /// How many of `lights` are lamps. The eyes of the strokes that
    /// reveal come after them.
    lamps: usize,
    /// The triangles the strokes that reveal cut out of the darkness: x,
    /// y, the row of the eye that opens them, and 1 for a triangle that
    /// conceals instead.
    reveal: Vec<[f32; 4]>,
    /// Whether the strokes that conceal are all the scene holds, so the
    /// rest of the map shows as it is.
    plain: bool,
}

/// The most rows the shadow map holds, lamps and eyes together. A texture
/// may be 8192 texels high on every GPU wgpu runs on.
const MOST_ROWS: usize = 4096;

/// The world rectangle a camera shows on a surface of `viewport` pixels.
pub fn view_of(camera: &Camera, viewport: (u32, u32)) -> ((f64, f64), (f64, f64)) {
    let min = camera.screen_to_world((0.0, 0.0), viewport);
    let max = camera.screen_to_world((f64::from(viewport.0), f64::from(viewport.1)), viewport);
    (
        (min.0.min(max.0), min.1.min(max.1)),
        (min.0.max(max.0), min.1.max(max.1)),
    )
}

/// The light one screen draws this frame, or `None` for no light.
///
/// `reveal` holds what the strokes that reveal open, from
/// [`crate::ink::reveal_mesh`]. A scene with no light still goes dark
/// when it holds one of them, so the stroke has a darkness to cut.
pub fn input_for(
    scene: &Scene,
    audience: Audience,
    rules: Rules,
    show: Show,
    camera: &Camera,
    viewport: (u32, u32),
    reveal: &Reveal,
) -> Option<Input> {
    if !show.on(audience) {
        return None;
    }
    gather(scene, audience, rules, view_of(camera, viewport), reveal)
}

/// The lights, the walls and the dark areas that reach a view, or `None`
/// when the scene holds no light and no dark area for this screen.
///
/// A light counts when its dim reach touches the view. A wall counts when
/// it touches the reach of a light that counts, so a wall off the screen
/// still casts a shadow onto it. PLAN.md 8.1.
///
/// It also takes the eyes and the triangles of the strokes that reveal.
/// An eye counts the walls that stop sight, whatever the rules of the
/// light say, because it stands for what the players see. A closed door
/// stops it and an open one does not.
pub fn gather(
    scene: &Scene,
    audience: Audience,
    rules: Rules,
    view: ((f64, f64), (f64, f64)),
    reveal: &Reveal,
) -> Option<Input> {
    let nodes = crate::scene::sight_order(scene, audience);
    let all: Vec<&Light> = nodes
        .iter()
        .filter_map(|node| match node {
            Node::Light(light) => Some(light),
            _ => None,
        })
        .collect();
    let areas: Vec<&Darkness> = nodes
        .iter()
        .filter_map(|node| match node {
            Node::Darkness(area) => Some(area),
            _ => None,
        })
        .collect();
    let plain = all.is_empty() && areas.is_empty() && reveal.eyes.is_empty();
    if plain && reveal.mesh.is_empty() {
        return None;
    }
    let lights: Vec<&Light> = all
        .into_iter()
        .filter(|light| light.reach() > 0.0 && touches(light, view))
        .collect();
    let blocks = |wall: &&Wall| {
        rules.walls_block && wall.blocks_light && !(rules.open_doors_pass && wall.is_open())
    };
    let segments = || {
        nodes
            .iter()
            .filter_map(|node| match node {
                Node::Walls(walls) => Some(&walls.segments),
                _ => None,
            })
            .flatten()
    };
    let blocking: Vec<&Wall> = segments().filter(blocks).collect();
    // Each light gets its own run of the walls it reaches, so the shadow
    // pass tests a light against those walls only.
    let mut walls: Vec<GpuWall> = Vec::new();
    let mut run = |light: &Light, near: &dyn Fn(&Wall) -> bool, from: &[&Wall]| {
        let first = walls.len();
        walls.extend(
            from.iter()
                .filter(|wall| near(wall))
                .map(|wall| [wall.a.0, wall.a.1, wall.b.0, wall.b.1].map(|v| v as f32)),
        );
        let mut packed = pack(light);
        packed[10] = f32::from_bits(first as u32);
        packed[11] = f32::from_bits((walls.len() - first) as u32);
        packed
    };
    let mut lights: Vec<GpuLight> = lights
        .iter()
        .map(|light| run(light, &|wall| near(wall, light), &blocking))
        .collect();
    let lamps = lights.len();
    // The eyes come after the lamps. A scene with more of them than the
    // shadow map has rows for drops the last ones, and what they open.
    let sight: Vec<&Wall> = segments()
        .filter(|wall| wall.blocks_sight && !wall.is_open())
        .collect();
    let eyes = reveal.eyes.len().min(MOST_ROWS.saturating_sub(lamps));
    for eye in &reveal.eyes[..eyes] {
        let light = Light::new(0, eye.center, eye.reach, eye.reach);
        lights.push(run(&light, &|wall| near(wall, &light), &sight));
    }
    let reveal = reveal
        .mesh
        .iter()
        .filter(|(_, eye)| *eye == CONCEAL || (*eye as usize) < eyes)
        .map(|(at, eye)| match *eye {
            CONCEAL => [at[0], at[1], 0.0, f32::from_bits(1)],
            eye => [at[0], at[1], f32::from_bits(lamps as u32 + eye), 0.0],
        })
        .collect();
    // The areas keep the order of the tree, so the last one that holds a
    // point is the one on top there.
    let areas: Vec<&Darkness> = areas
        .into_iter()
        .filter(|area| overlaps(area.bounds(), view))
        .collect();
    let fills = areas.iter().any(|area| area.fill != Fill::Black);
    let zones = areas.iter().map(|area| pack_zone(area)).collect();
    Some(Input {
        lights,
        walls,
        zones,
        fills,
        lamps,
        reveal,
        plain,
    })
}

/// Whether two boxes share a point.
fn overlaps(a: ((f64, f64), (f64, f64)), b: ((f64, f64), (f64, f64))) -> bool {
    a.0.0 <= b.1.0 && b.0.0 <= a.1.0 && a.0.1 <= b.1.1 && b.0.1 <= a.1.1
}

fn pack_zone(area: &Darkness) -> GpuZone {
    let [a, b, c, d] = area.corners.map(|(x, y)| [x as f32, y as f32]);
    [
        a[0],
        a[1],
        b[0],
        b[1],
        c[0],
        c[1],
        d[0],
        d[1],
        area.strength.clamp(0.0, 1.0) as f32,
        f32::from_bits(area.fill.code()),
        0.0,
        0.0,
    ]
}

/// Whether the reach of `light` touches the rectangle `view`.
fn touches(light: &Light, (min, max): ((f64, f64), (f64, f64))) -> bool {
    let near = (
        light.center.0.clamp(min.0, max.0),
        light.center.1.clamp(min.1, max.1),
    );
    (near.0 - light.center.0).hypot(near.1 - light.center.1) <= light.reach()
}

/// Whether `wall` comes within the reach of `light`.
fn near(wall: &Wall, light: &Light) -> bool {
    wall.distance(light.center) <= light.reach()
}

fn pack(light: &Light) -> GpuLight {
    let [r, g, b] = light.color.map(|c| f32::from(c) / 255.0);
    [
        light.center.0 as f32,
        light.center.1 as f32,
        light.bright as f32,
        light.dim as f32,
        r,
        g,
        b,
        0.0,
        light.angle as f32,
        light.rotation as f32,
        0.0,
        0.0,
    ]
}

/// The shadow map: for each light row and direction column, the distance
/// to the nearest wall, or the reach of the light when no wall stands
/// in the way.
const SHADOW_SHADER: &str = "
struct Light {
    center: vec2<f32>,
    bright: f32,
    dim: f32,
    color: vec4<f32>,
    angle: f32,
    rotation: f32,
    /// Where the walls of this light start in `walls`, and how many.
    first_wall: u32,
    wall_count: u32,
};

struct Counts {
    lights: u32,
    walls: u32,
    samples: u32,
    pad: u32,
};

@group(0) @binding(0) var<uniform> counts: Counts;
@group(0) @binding(1) var<storage, read> lights: array<Light>;
@group(0) @binding(2) var<storage, read> walls: array<vec4<f32>>;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

fn cross2(a: vec2<f32>, b: vec2<f32>) -> f32 {
    return a.x * b.y - a.y * b.x;
}

@fragment
fn fs(@builtin(position) at: vec4<f32>) -> @location(0) f32 {
    let row = u32(at.y);
    let light = lights[row];
    let reach = max(light.bright, light.dim);
    let turn = (at.x / f32(counts.samples)) * 6.28318530718 - 3.14159265359;
    let ray = vec2<f32>(cos(turn), sin(turn));
    var nearest = reach;
    let end = light.first_wall + light.wall_count;
    for (var i = light.first_wall; i < end; i = i + 1u) {
        let wall = walls[i];
        let a = wall.xy - light.center;
        let b = wall.zw - light.center;
        let edge = b - a;
        let across = cross2(ray, edge);
        if (abs(across) < 1e-9) {
            continue;
        }
        // Where the ray meets the line of the wall, and where along the
        // wall that point lies.
        let t = cross2(a, edge) / across;
        let s = cross2(a, ray) / across;
        if (t > 0.0 && s >= 0.0 && s <= 1.0) {
            nearest = min(nearest, t);
        }
    }
    return nearest;
}
";

/// The light over the screen. It writes a factor that multiplies the
/// maps under it.
const LIGHT_SHADER: &str = "
struct Light {
    center: vec2<f32>,
    bright: f32,
    dim: f32,
    color: vec4<f32>,
    angle: f32,
    rotation: f32,
    pad_a: f32,
    pad_b: f32,
};

struct Zone {
    a: vec2<f32>,
    b: vec2<f32>,
    c: vec2<f32>,
    d: vec2<f32>,
    darkness: f32,
    fill: u32,
    pad_a: f32,
    pad_b: f32,
};

struct View {
    viewport: vec2<f32>,
    center: vec2<f32>,
    pixels_per_inch: f32,
    lights: u32,
    samples: u32,
    zones: u32,
    /// The ambient color, and the darkness of the scene in the alpha.
    ambient: vec4<f32>,
    /// How much of the darkness this screen keeps: all of it on the TV,
    /// less on the DM screen, so the DM sees the map under it.
    keep: f32,
    /// 1 on the DM screen, which marks what the players cannot see, and 0
    /// on the TV.
    marks: f32,
    pad_d: f32,
    pad_e: f32,
};

@group(0) @binding(0) var<uniform> view: View;
@group(0) @binding(1) var<storage, read> lights: array<Light>;
@group(0) @binding(2) var shadows: texture_2d<f32>;
@group(0) @binding(3) var<storage, read> zones: array<Zone>;
@group(0) @binding(4) var lamp_map: texture_2d<f32>;
@group(0) @binding(5) var lamp_sampler: sampler;
/// One point of a reveal triangle, the row of the eye that opens it, and
/// 1 when the triangle conceals instead.
struct Spot {
    at: vec2<f32>,
    eye: u32,
    hide: u32,
};

@group(0) @binding(6) var<storage, read> reveal: array<Spot>;
@group(0) @binding(7) var reveal_mask: texture_2d<f32>;

/// How many screen pixels one texel of `lamp_map` covers, each way. The
/// Rust `LAMP_SCALE` holds the same number.
const SCALE: f32 = 4.0;

/// How much of its own color a lamp lends to what it lights. A lamp of a
/// strong color still shows the map under it.
const TINT: f32 = 0.5;

/// The light a lamp gives in its dim ring, against its bright core.
const DIM: f32 = 0.5;

/// How far past a wall a point still takes the light, in inches, so the
/// face of the wall itself is lit.
const BIAS: f32 = 0.02;

/// How wide the soft edge of a shadow is, in inches, at the least.
const SOFT: f32 = 0.25;

/// How many lamp texels the soft edge of a shadow spans, at the least, so
/// a zoom far out still shows no steps.
const SOFT_TEXELS: f32 = 3.0;

/// How many directions the soft shadow test reads.
const TAPS: i32 = 5;

/// The color the DM screen lays over what the players cannot see. It is
/// a cold blue, so it never reads as the color of a map.
const FOG_TINT: vec3<f32> = vec3<f32>(0.8, 0.87, 1.0);

/// How dark the hatch lines over the fog are, and how many screen pixels
/// lie between two of them.
const HATCH: f32 = 0.7;
const HATCH_PX: f32 = 12.0;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

fn world_of(at: vec4<f32>) -> vec2<f32> {
    return view.center + (at.xy - view.viewport * 0.5) / view.pixels_per_inch;
}

fn cross2(a: vec2<f32>, b: vec2<f32>) -> f32 {
    return a.x * b.y - a.y * b.x;
}

/// Whether `p` lies inside the convex quad of `zone`, whichever way its
/// corners run.
fn inside(zone: Zone, p: vec2<f32>) -> bool {
    let s0 = cross2(zone.b - zone.a, p - zone.a);
    let s1 = cross2(zone.c - zone.b, p - zone.b);
    let s2 = cross2(zone.d - zone.c, p - zone.c);
    let s3 = cross2(zone.a - zone.d, p - zone.d);
    let low = min(min(s0, s1), min(s2, s3));
    let high = max(max(s0, s1), max(s2, s3));
    return low >= 0.0 || high <= 0.0;
}

/// The index of the topmost area that holds `p`, or the count of areas
/// when none does.
fn zone_at(p: vec2<f32>) -> u32 {
    var found = view.zones;
    for (var i = 0u; i < view.zones; i = i + 1u) {
        if (inside(zones[i], p)) {
            found = i;
        }
    }
    return found;
}

/// The light lamp `i` gives at `world`, before the darkness.
fn lamp(i: u32, world: vec2<f32>) -> vec3<f32> {
    let light = lights[i];
    let reach = max(light.bright, light.dim);
    let away = world - light.center;
    let distance = length(away);
    if (distance > reach) {
        return vec3<f32>(0.0);
    }
    var seen = sight(i, away, distance);
    if (light.angle < 6.28 && distance > 0.0) {
        // Zero points down the screen, and the turn runs clockwise. The
        // side of the cone fades over the same soft band as a shadow.
        let facing = vec2<f32>(-sin(light.rotation), cos(light.rotation));
        let off = acos(clamp(dot(away / distance, facing), -1.0, 1.0));
        let spread = soft_width() / max(distance, soft_width());
        let half = light.angle * 0.5;
        seen = seen * (1.0 - smoothstep(half - spread * 0.5, half + spread * 0.5, off));
    }
    if (seen <= 0.0) {
        return vec3<f32>(0.0);
    }
    var strength = DIM * (1.0 - smoothstep(mix(light.bright, reach, 0.7), reach, distance));
    if (distance <= light.bright) {
        strength = 1.0;
    }
    return seen * strength * mix(vec3<f32>(1.0), light.color.rgb, TINT);
}

/// How wide the soft edge of a shadow is at this zoom, in inches.
fn soft_width() -> f32 {
    return max(SOFT, SOFT_TEXELS * SCALE / view.pixels_per_inch);
}

/// How much of lamp `i` reaches a point `away` from it, `distance` off,
/// from 0 in the shadow of a wall to 1 in the open.
///
/// The test reads the shadow map in `TAPS` directions that span `soft`
/// inches across, and each one fades out over half of `soft` past its
/// wall. The edge of a shadow is then a soft band some texels wide, and
/// the lamp texture, a quarter of the screen size, draws it with no steps.
fn sight(i: u32, away: vec2<f32>, distance: f32) -> f32 {
    let soft = soft_width();
    let turn = atan2(away.y, away.x);
    let spread = soft / max(distance, soft);
    var seen = 0.0;
    for (var k = 0; k < TAPS; k = k + 1) {
        let offset = (f32(k) / f32(TAPS - 1) - 0.5) * spread;
        let round = fract((turn + offset) / 6.28318530718 + 0.5);
        let column = min(u32(round * f32(view.samples)), view.samples - 1u);
        let wall = textureLoad(shadows, vec2<u32>(column, i), 0).r + BIAS;
        seen = seen + 1.0 - smoothstep(wall, wall + soft * 0.5, distance);
    }
    return seen / f32(TAPS);
}

/// The darkness at `world`: the topmost area's, or the scene's.
fn darkness_at(world: vec2<f32>, zone: u32) -> f32 {
    if (zone < view.zones) {
        return zones[zone].darkness;
    }
    return view.ambient.a;
}

/// Turns a value the eye reads into the linear light an sRGB surface
/// takes. The darkness, the strengths and the colors here are all in the
/// first kind, so 50 % darkness looks half as dark.
fn linear(c: vec3<f32>) -> vec3<f32> {
    return pow(max(c, vec3<f32>(0.0)), vec3<f32>(2.2));
}

struct Lamp {
    @builtin(position) at: vec4<f32>,
    @location(0) @interpolate(flat) index: u32,
};

/// The lamp pass draws one box for each lamp: the square of its reach.
@vertex
fn vs_lamp(@builtin(vertex_index) v: u32, @builtin(instance_index) i: u32) -> Lamp {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0),
        vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
    );
    let light = lights[i];
    let world = light.center + corners[v] * max(light.bright, light.dim);
    // The lamp texture covers whole texels, so it may reach a few pixels
    // past the screen.
    let size = ceil(view.viewport / SCALE) * SCALE;
    let pixel = (world - view.center) * view.pixels_per_inch + view.viewport * 0.5;
    let clip = pixel / size * 2.0 - 1.0;
    return Lamp(vec4<f32>(clip.x, -clip.y, 0.0, 1.0), i);
}

/// The lamp pass: the light of one lamp, on a texture `SCALE` times
/// smaller than the screen. The lamps add up.
@fragment
fn fs_lamps(lamp_in: Lamp) -> @location(0) vec4<f32> {
    let at = lamp_in.at.xy * SCALE;
    let world = view.center + (at - view.viewport * 0.5) / view.pixels_per_inch;
    return vec4<f32>(lamp(lamp_in.index, world), 1.0);
}

struct Opening {
    @builtin(position) at: vec4<f32>,
    @location(0) @interpolate(flat) eye: u32,
    @location(1) @interpolate(flat) hide: u32,
};

/// The reveal pass draws the triangles of the strokes that reveal onto
/// the mask, one texel for each pixel of the screen.
@vertex
fn vs_reveal(@builtin(vertex_index) v: u32) -> Opening {
    let spot = reveal[v];
    let pixel = (spot.at - view.center) * view.pixels_per_inch + view.viewport * 0.5;
    let clip = pixel / view.viewport * 2.0 - 1.0;
    return Opening(vec4<f32>(clip.x, -clip.y, 0.0, 1.0), spot.eye, spot.hide);
}

/// A triangle opens what its eye sees past the walls. The red channel of
/// the mask holds what is open, the green one what is concealed.
///
/// The mask blends as alpha over what is there, in the order of the
/// strokes. A reveal of strength `s` takes `s` of the red and lets go of
/// `s` of the green. A triangle that conceals sets the green and clears
/// the red.
@fragment
fn fs_reveal(opening: Opening) -> @location(0) vec4<f32> {
    if (opening.hide == 1u) {
        return vec4<f32>(0.0, 1.0, 0.0, 1.0);
    }
    let away = world_of(opening.at) - lights[opening.eye].center;
    let seen = sight(opening.eye, away, length(away));
    return vec4<f32>(seen, 0.0, 0.0, seen);
}

/// 1 where a stroke reveals the screen pixel `at`, else 0.
fn revealed(at: vec4<f32>) -> f32 {
    return textureLoad(reveal_mask, vec2<u32>(at.xy), 0).r;
}

/// 1 where a stroke conceals the screen pixel `at`, else 0.
fn concealed(at: vec4<f32>) -> f32 {
    return textureLoad(reveal_mask, vec2<u32>(at.xy), 0).g;
}

/// The lamp light at the screen pixel `at`, from the lamp pass.
fn lamps_near(at: vec4<f32>) -> vec3<f32> {
    let size = vec2<f32>(textureDimensions(lamp_map)) * SCALE;
    return textureSampleLevel(lamp_map, lamp_sampler, at.xy / size, 0.0).rgb;
}

/// The light pass: a factor that multiplies the maps under it.
@fragment
fn fs(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let world = world_of(at);
    // A stroke that conceals makes the map fully dark, and puts out the
    // lamps there too.
    let hide = concealed(at);
    let darkness = mix(darkness_at(world, zone_at(world)), 1.0, hide);
    // A stroke that reveals lights the map in full, as a bright lamp does.
    let lit = lamps_near(at) * (1.0 - hide) + revealed(at);
    let players = min(view.ambient.rgb * (1.0 - darkness) + lit, vec3<f32>(1.0));
    if (view.marks < 0.5) {
        return vec4<f32>(linear(players), 1.0);
    }
    // The DM sees through a part of the darkness, and a blue hatch marks
    // all that the players miss, as strong as the dark is there.
    let kept = darkness * view.keep;
    let seen = min(view.ambient.rgb * (1.0 - kept) + lit, vec3<f32>(1.0));
    let hidden = 1.0 - max(max(players.r, players.g), players.b);
    let line = fract((at.x + at.y) / HATCH_PX) < 0.17;
    let hatch = select(1.0, HATCH, line);
    let mark = mix(vec3<f32>(1.0), FOG_TINT * hatch, hidden);
    return vec4<f32>(linear(seen * mark), 1.0);
}

fn hash(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(123.34, 456.21));
    let r = q + dot(q, q + 45.32);
    return fract(r.x * r.y);
}

/// Smooth value noise, from 0 to 1.
fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash(i);
    let b = hash(i + vec2<f32>(1.0, 0.0));
    let c = hash(i + vec2<f32>(0.0, 1.0));
    let d = hash(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

/// Five layers of noise, each half the size of the last.
fn fbm(p: vec2<f32>) -> f32 {
    var sum = 0.0;
    var weight = 0.5;
    var q = p;
    for (var i = 0; i < 5; i = i + 1) {
        sum = sum + weight * noise(q);
        q = q * 2.03 + vec2<f32>(17.1, 9.2);
        weight = weight * 0.5;
    }
    return sum;
}

/// The pattern of a fill at `world`, in inches. The sizes are in cells,
/// so a cloud covers a few cells at any zoom.
fn pattern(fill: u32, world: vec2<f32>) -> vec3<f32> {
    switch (fill) {
        case 1u: {
            let n = fbm(world / 6.0);
            return mix(vec3<f32>(0.45, 0.48, 0.52), vec3<f32>(0.95, 0.95, 0.97), smoothstep(0.3, 0.7, n));
        }
        case 2u: {
            let n = fbm(world / 4.0 + vec2<f32>(fbm(world / 9.0) * 3.0));
            return mix(vec3<f32>(0.04, 0.035, 0.03), vec3<f32>(0.32, 0.29, 0.26), n);
        }
        case 3u: {
            let n = fbm(world / 8.0);
            return mix(vec3<f32>(0.32, 0.38, 0.45), vec3<f32>(0.72, 0.78, 0.84), n);
        }
        case 4u: {
            let sky = mix(vec3<f32>(0.01, 0.015, 0.04), vec3<f32>(0.05, 0.06, 0.14), fbm(world / 10.0));
            let cell = floor(world * 2.0);
            let spot = fract(world * 2.0) - vec2<f32>(hash(cell), hash(cell + 7.0));
            let star = step(0.93, hash(cell + 3.0)) * smoothstep(0.08, 0.0, length(spot));
            return sky + vec3<f32>(star);
        }
        default: {
            return vec3<f32>(0.0);
        }
    }
}

struct Patch {
    @builtin(position) at: vec4<f32>,
    @location(0) @interpolate(flat) zone: u32,
};

/// The fill pass draws the quad of each area. An area with no fill draws
/// nothing.
@vertex
fn vs_fill(@builtin(vertex_index) v: u32, @builtin(instance_index) i: u32) -> Patch {
    let zone = zones[i];
    var corners = array<vec2<f32>, 6>(zone.a, zone.b, zone.c, zone.a, zone.c, zone.d);
    var world = corners[v];
    if (zone.fill == 0u) {
        world = zone.a;
    }
    let pixel = (world - view.center) * view.pixels_per_inch + view.viewport * 0.5;
    let clip = pixel / view.viewport * 2.0 - 1.0;
    return Patch(vec4<f32>(clip.x, -clip.y, 0.0, 1.0), i);
}

/// The fill pass: the pattern of an area, over the part of it no lamp
/// lights. It goes down after the light pass, with alpha blending.
@fragment
fn fs_fill(piece: Patch) -> @location(0) vec4<f32> {
    let at = piece.at;
    let world = world_of(at);
    let zone = zone_at(world);
    // Where areas overlap, only the one on top draws.
    if (zone != piece.zone) {
        discard;
    }
    let lit = lamps_near(at) * (1.0 - concealed(at));
    let light = min(max(max(max(lit.r, lit.g), lit.b), revealed(at)), 1.0);
    let cover = zones[zone].darkness * view.keep * (1.0 - light);
    return vec4<f32>(linear(pattern(zones[zone].fill, world)), cover);
}
";

/// How many bytes the `View` block of the light shader holds: eight
/// numbers of four bytes, then the ambient color and the darkness, then
/// what the screen keeps of the darkness and three pads.
const VIEW_BYTES: u64 = 64;

/// The GPU side of the light for one screen.
///
/// The DM screen and the TV each hold one, because each shows its own
/// lights over its own part of the canvas.
pub struct LightLayer {
    shadow_pipeline: wgpu::RenderPipeline,
    lamp_pipeline: wgpu::RenderPipeline,
    reveal_pipeline: wgpu::RenderPipeline,
    light_pipeline: wgpu::RenderPipeline,
    fill_pipeline: wgpu::RenderPipeline,
    shadow_layout: wgpu::BindGroupLayout,
    light_layout: wgpu::BindGroupLayout,
    counts: wgpu::Buffer,
    view: wgpu::Buffer,
    lights: wgpu::Buffer,
    walls: wgpu::Buffer,
    zones: wgpu::Buffer,
    /// The triangles of the strokes that reveal.
    reveal: wgpu::Buffer,
    shadows: wgpu::TextureView,
    /// The lamp light of this frame, and its size in texels.
    lamps: (wgpu::TextureView, (u32, u32)),
    /// What the lamp pass binds in the place of `lamps`. A pass may not
    /// read the texture it draws on.
    no_lamps: wgpu::TextureView,
    /// Where the strokes that reveal lie, at the size of the screen.
    mask: (wgpu::TextureView, (u32, u32)),
    /// What the lamp and the reveal passes bind in the place of `mask`.
    no_mask: wgpu::TextureView,
    sampler: wgpu::Sampler,
    shadow_group: wgpu::BindGroup,
    lamp_group: wgpu::BindGroup,
    light_group: wgpu::BindGroup,
    /// How many lights the buffer and the shadow map have room for.
    light_room: usize,
    /// How many walls the buffer has room for.
    wall_room: usize,
    /// How many areas of darkness the buffer has room for.
    zone_room: usize,
    /// How many points of the reveal triangles the buffer has room for.
    reveal_room: usize,
    /// The lights and the walls the shadow map holds now, so an unchanged
    /// frame skips it.
    drawn: Option<(Vec<GpuLight>, Vec<GpuWall>)>,
    /// Whether this frame draws light, and how many areas the fill pass
    /// draws, or none when no area has a fill.
    draws: Option<u32>,
}

impl std::fmt::Debug for LightLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LightLayer").finish_non_exhaustive()
    }
}

impl LightLayer {
    /// Builds the five pipelines for surfaces of `format`.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let (shadow_layout, light_layout) = layouts(device);
        let [
            shadow_pipeline,
            lamp_pipeline,
            reveal_pipeline,
            light_pipeline,
            fill_pipeline,
        ] = pipelines(device, format, &shadow_layout, &light_layout);
        let counts = buffer(device, "light counts", 16, wgpu::BufferUsages::UNIFORM);
        let view = buffer(
            device,
            "light view",
            VIEW_BYTES,
            wgpu::BufferUsages::UNIFORM,
        );
        let lights = storage_buffer::<GpuLight>(device, "lights", 1);
        let walls = storage_buffer::<GpuWall>(device, "walls", 1);
        let zones = storage_buffer::<GpuZone>(device, "zones", 1);
        let reveal = storage_buffer::<[f32; 4]>(device, "reveal", 1);
        let shadows = shadow_texture(device, 1);
        let lamps = lamp_texture(device, (1, 1));
        let no_lamps = lamp_texture(device, (1, 1));
        let mask = mask_texture(device, (1, 1));
        let no_mask = mask_texture(device, (1, 1));
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("lamps"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..wgpu::SamplerDescriptor::default()
        });
        // The groups take their real buffers in `regroup`.
        let shadow_group = bind(
            device,
            &shadow_layout,
            &[
                counts.as_entire_binding(),
                lights.as_entire_binding(),
                walls.as_entire_binding(),
            ],
        );
        let light_group = bind_light(
            device,
            &light_layout,
            [&view, &lights, &zones, &reveal],
            [&shadows, &no_lamps, &no_mask],
            &sampler,
        );
        let mut layer = Self {
            shadow_pipeline,
            lamp_pipeline,
            reveal_pipeline,
            light_pipeline,
            fill_pipeline,
            shadow_layout,
            light_layout,
            counts,
            view,
            lights,
            walls,
            zones,
            reveal,
            shadows,
            lamps: (lamps, (1, 1)),
            no_lamps,
            mask: (mask, (1, 1)),
            no_mask,
            sampler,
            shadow_group,
            lamp_group: light_group.clone(),
            light_group,
            light_room: 1,
            wall_room: 1,
            zone_room: 1,
            reveal_room: 1,
            drawn: None,
            draws: None,
        };
        layer.regroup(device);
        layer
    }

    /// Gets the light of one frame ready, before the frame draws.
    ///
    /// `input` is what [`input_for`] found, or `None` for a frame with no
    /// light. `keep` is how much of the darkness this screen shows, from 0
    /// to 1, and `marks` whether it marks what the players cannot see. The shadow map draws again only when the lights or the walls
    /// changed, so a still scene costs a full-screen pass or two and
    /// nothing more.
    #[expect(
        clippy::too_many_arguments,
        reason = "one frame of light needs each of them"
    )]
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        input: Option<Input>,
        mood: Mood,
        (keep, marks): (f64, bool),
        camera: &Camera,
        viewport: (u32, u32),
    ) {
        let Some(input) = input else {
            self.draws = None;
            return;
        };
        self.room(device, &input);
        let size = (
            viewport.0.div_ceil(LAMP_SCALE).max(1),
            viewport.1.div_ceil(LAMP_SCALE).max(1),
        );
        let full = (viewport.0.max(1), viewport.1.max(1));
        if size != self.lamps.1 || full != self.mask.1 {
            self.lamps = (lamp_texture(device, size), size);
            self.mask = (mask_texture(device, full), full);
            self.regroup(device);
        }
        // A scene that only conceals keeps the rest of its map as it is.
        let ([r, g, b], darkness) = if input.plain {
            ([1.0; 3], 0.0)
        } else {
            (
                mood.ambient.map(|c| f32::from(c) / 255.0),
                mood.darkness.clamp(0.0, 1.0) as f32,
            )
        };
        // The block mixes floats and counts, in the order of `View`.
        let mut view = bytes(&[
            viewport.0 as f32,
            viewport.1 as f32,
            camera.center.0 as f32,
            camera.center.1 as f32,
            camera.pixels_per_inch as f32,
        ]);
        for count in [input.lights.len(), SAMPLES as usize, input.zones.len()] {
            view.extend((count as u32).to_ne_bytes());
        }
        view.extend(bytes(&[r, g, b, darkness]));
        let marks = if marks { 1.0 } else { 0.0 };
        view.extend(bytes(&[keep.clamp(0.0, 1.0) as f32, marks, 0.0, 0.0]));
        queue.write_buffer(&self.view, 0, &view);
        queue.write_buffer(&self.zones, 0, &bytes(input.zones.as_flattened()));
        queue.write_buffer(&self.reveal, 0, &bytes(input.reveal.as_flattened()));
        self.draws = Some(if input.fills {
            input.zones.len() as u32
        } else {
            0
        });
        let same = self
            .drawn
            .as_ref()
            .is_some_and(|(lights, walls)| *lights == input.lights && *walls == input.walls);
        let rows = input.lights.len();
        let (lamps, points) = (input.lamps as u32, input.reveal.len() as u32);
        if !same && rows > 0 {
            self.draw_shadows(device, queue, &input);
            self.drawn = Some((input.lights, input.walls));
        }
        self.draw_lamps(device, queue, lamps, points);
    }

    /// Draws the light of the `count` lamps of this frame onto `lamps`,
    /// and the `points` of the reveal triangles onto `mask`.
    fn draw_lamps(&self, device: &wgpu::Device, queue: &wgpu::Queue, count: u32, points: u32) {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("lamps"),
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("lamps"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.lamps.0,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.lamp_pipeline);
        pass.set_bind_group(0, &self.lamp_group, &[]);
        pass.draw(0..6, 0..count);
        drop(pass);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("reveal"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.mask.0,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if points > 0 {
            pass.set_pipeline(&self.reveal_pipeline);
            pass.set_bind_group(0, &self.lamp_group, &[]);
            pass.draw(0..points, 0..1);
        }
        drop(pass);
        queue.submit([encoder.finish()]);
    }

    /// Draws the shadow map of the lights and walls in `input`.
    fn draw_shadows(&self, device: &wgpu::Device, queue: &wgpu::Queue, input: &Input) {
        queue.write_buffer(&self.lights, 0, &bytes(input.lights.as_flattened()));
        queue.write_buffer(&self.walls, 0, &bytes(input.walls.as_flattened()));
        let counts = [input.lights.len(), input.walls.len(), SAMPLES as usize, 0];
        let counts: Vec<u8> = counts
            .iter()
            .flat_map(|c| (*c as u32).to_ne_bytes())
            .collect();
        queue.write_buffer(&self.counts, 0, &counts);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("shadow"),
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shadow"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.shadows,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        // Only the rows of this frame's lights draw. The rows past them
        // hold nothing the light pass reads.
        let rows = input.lights.len() as f32;
        pass.set_viewport(0.0, 0.0, SAMPLES as f32, rows, 0.0, 1.0);
        pass.set_pipeline(&self.shadow_pipeline);
        pass.set_bind_group(0, &self.shadow_group, &[]);
        pass.draw(0..3, 0..1);
        // The pass holds the encoder until it goes.
        drop(pass);
        queue.submit([encoder.finish()]);
    }

    /// Lays the light of this frame over the maps in `pass`, then the
    /// fill of each area of darkness.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        let Some(areas) = self.draws else {
            return;
        };
        pass.set_bind_group(0, &self.light_group, &[]);
        pass.set_pipeline(&self.light_pipeline);
        pass.draw(0..3, 0..1);
        if areas > 0 {
            pass.set_pipeline(&self.fill_pipeline);
            pass.draw(0..6, 0..areas);
        }
    }

    /// Grows the buffers and the shadow map to hold what `input` holds, in
    /// doublings.
    fn room(&mut self, device: &wgpu::Device, input: &Input) {
        let (lights, walls, zones) = (input.lights.len(), input.walls.len(), input.zones.len());
        let points = input.reveal.len();
        if lights <= self.light_room
            && walls <= self.wall_room
            && zones <= self.zone_room
            && points <= self.reveal_room
        {
            return;
        }
        if lights > self.light_room {
            self.light_room = lights.next_power_of_two();
            self.lights = storage_buffer::<GpuLight>(device, "lights", self.light_room);
            self.shadows = shadow_texture(device, self.light_room as u32);
        }
        if walls > self.wall_room {
            self.wall_room = walls.next_power_of_two();
            self.walls = storage_buffer::<GpuWall>(device, "walls", self.wall_room);
        }
        if zones > self.zone_room {
            self.zone_room = zones.next_power_of_two();
            self.zones = storage_buffer::<GpuZone>(device, "zones", self.zone_room);
        }
        if points > self.reveal_room {
            self.reveal_room = points.next_power_of_two();
            self.reveal = storage_buffer::<[f32; 4]>(device, "reveal", self.reveal_room);
        }
        self.regroup(device);
        // The new buffers hold nothing yet, so the next frame draws the
        // shadow map whatever it held before.
        self.drawn = None;
    }

    /// Binds the buffers and the textures again, after one of them grew.
    fn regroup(&mut self, device: &wgpu::Device) {
        self.shadow_group = bind(
            device,
            &self.shadow_layout,
            &[
                self.counts.as_entire_binding(),
                self.lights.as_entire_binding(),
                self.walls.as_entire_binding(),
            ],
        );
        let buffers = [&self.view, &self.lights, &self.zones, &self.reveal];
        self.lamp_group = bind_light(
            device,
            &self.light_layout,
            buffers,
            [&self.shadows, &self.no_lamps, &self.no_mask],
            &self.sampler,
        );
        self.light_group = bind_light(
            device,
            &self.light_layout,
            buffers,
            [&self.shadows, &self.lamps.0, &self.mask.0],
            &self.sampler,
        );
    }
}

/// The bind group layouts of the shadow pass and of the light passes.
///
/// Both read a uniform and the lights. The shadow pass reads the walls
/// as well. The light passes read the shadow map, the dark areas and the
/// lamp light.
fn layouts(device: &wgpu::Device) -> (wgpu::BindGroupLayout, wgpu::BindGroupLayout) {
    let storage = |binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    let uniform = wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("shadow"),
        entries: &[uniform, storage(1), storage(2)],
    });
    let light_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("light"),
        entries: &[
            uniform,
            storage(1),
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            storage(3),
            storage(6),
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 7,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    });
    (shadow_layout, light_layout)
}

/// The shadow, lamp, reveal, light and fill pipelines, in that order.
fn pipelines(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    shadow_layout: &wgpu::BindGroupLayout,
    light_layout: &wgpu::BindGroupLayout,
) -> [wgpu::RenderPipeline; 5] {
    let shadow_target = wgpu::ColorTargetState {
        format: wgpu::TextureFormat::R32Float,
        blend: None,
        write_mask: wgpu::ColorWrites::ALL,
    };
    let shadow_pipeline = pipeline(
        device,
        SHADOW_SHADER,
        ("vs", "fs"),
        shadow_layout,
        shadow_target,
    );
    let add = wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    };
    let lamp_target = wgpu::ColorTargetState {
        format: LAMP_FORMAT,
        blend: Some(wgpu::BlendState {
            color: add,
            alpha: add,
        }),
        write_mask: wgpu::ColorWrites::ALL,
    };
    let lamp_pipeline = pipeline(
        device,
        LIGHT_SHADER,
        ("vs_lamp", "fs_lamps"),
        light_layout,
        lamp_target,
    );
    // Each triangle lies over the ones before it, so the last stroke over
    // a point decides it. See `fs_reveal`.
    let reveal_pipeline = pipeline(
        device,
        LIGHT_SHADER,
        ("vs_reveal", "fs_reveal"),
        light_layout,
        wgpu::ColorTargetState {
            format: MASK_FORMAT,
            blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        },
    );
    // The light multiplies what is under it: the result is the light
    // times the map, and the alpha of the map stays.
    let multiply = wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::Dst,
            dst_factor: wgpu::BlendFactor::Zero,
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::Zero,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        },
    };
    let target = |blend| wgpu::ColorTargetState {
        format,
        blend: Some(blend),
        write_mask: wgpu::ColorWrites::COLOR,
    };
    let light_pipeline = pipeline(
        device,
        LIGHT_SHADER,
        ("vs", "fs"),
        light_layout,
        target(multiply),
    );
    let fill_pipeline = pipeline(
        device,
        LIGHT_SHADER,
        ("vs_fill", "fs_fill"),
        light_layout,
        target(wgpu::BlendState::ALPHA_BLENDING),
    );
    [
        shadow_pipeline,
        lamp_pipeline,
        reveal_pipeline,
        light_pipeline,
        fill_pipeline,
    ]
}

/// A pipeline that draws with the vertex and fragment entries `entries`
/// of `source`.
fn pipeline(
    device: &wgpu::Device,
    source: &str,
    (vertex, entry): (&str, &str),
    layout: &wgpu::BindGroupLayout,
    target: wgpu::ColorTargetState,
) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(entry),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(entry),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(entry),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some(vertex),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some(entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(target)],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn buffer(
    device: &wgpu::Device,
    label: &str,
    size: u64,
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// A storage buffer with room for `count` items of `T`. A storage
/// binding may not be empty, so the caller asks for one item at least.
fn storage_buffer<T>(device: &wgpu::Device, label: &str, count: usize) -> wgpu::Buffer {
    buffer(
        device,
        label,
        (count.max(1) * std::mem::size_of::<T>()) as u64,
        wgpu::BufferUsages::STORAGE,
    )
}

/// The shadow map, with one row for each of `rows` lights.
fn shadow_texture(device: &wgpu::Device, rows: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("shadows"),
            size: wgpu::Extent3d {
                width: SAMPLES,
                height: rows.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

/// The format of the lamp light. A sum of lamps may pass 1.
const LAMP_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// The lamp light, `size` texels across and down.
fn lamp_texture(device: &wgpu::Device, size: (u32, u32)) -> wgpu::TextureView {
    target_texture(device, "lamps", LAMP_FORMAT, size)
}

/// The format of the reveal mask: red for what is open, green for what
/// is concealed, each from 0 to 1.
const MASK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg8Unorm;

fn mask_texture(device: &wgpu::Device, size: (u32, u32)) -> wgpu::TextureView {
    target_texture(device, "reveal mask", MASK_FORMAT, size)
}

/// A texture a pass draws on and a later pass reads.
fn target_texture(
    device: &wgpu::Device,
    label: &str,
    format: wgpu::TextureFormat,
    (width, height): (u32, u32),
) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

/// A bind group with one resource for each binding, in order.
fn bind(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    resources: &[wgpu::BindingResource<'_>],
) -> wgpu::BindGroup {
    let entries: Vec<wgpu::BindGroupEntry<'_>> = resources
        .iter()
        .enumerate()
        .map(|(binding, resource)| wgpu::BindGroupEntry {
            binding: binding as u32,
            resource: resource.clone(),
        })
        .collect();
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout,
        entries: &entries,
    })
}

/// The bind group of the light passes: the view, the lights, the shadow
/// map, the dark areas, the lamp light, the reveal triangles and their
/// mask.
fn bind_light(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    [view, lights, zones, reveal]: [&wgpu::Buffer; 4],
    [shadows, lamps, mask]: [&wgpu::TextureView; 3],
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    bind(
        device,
        layout,
        &[
            view.as_entire_binding(),
            lights.as_entire_binding(),
            wgpu::BindingResource::TextureView(shadows),
            zones.as_entire_binding(),
            wgpu::BindingResource::TextureView(lamps),
            wgpu::BindingResource::Sampler(sampler),
            reveal.as_entire_binding(),
            wgpu::BindingResource::TextureView(mask),
        ],
    )
}

fn bytes(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Reveal, Rules, gather};
    use crate::scene::{Audience, Node, Scene, Shown};
    use crate::sight::{Kind, Light, Wall, Walls};

    const LOOK: Rules = Rules {
        walls_block: true,
        open_doors_pass: true,
    };

    const VIEW: ((f64, f64), (f64, f64)) = ((-10.0, -10.0), (10.0, 10.0));

    fn scene(nodes: Vec<Node>) -> Scene {
        let mut scene = Scene::default();
        scene.root.children = nodes;
        scene
    }

    fn walls(segments: Vec<Wall>) -> Node {
        Node::Walls(Walls {
            id: 1,
            shown: Shown::default(),
            segments,
        })
    }

    #[test]
    fn a_dark_area_alone_still_draws_and_says_when_it_has_a_fill() {
        let mut area = crate::sight::Darkness::covering(3, ((-1.0, -1.0), (1.0, 1.0)), 1.0);
        let black = scene(vec![Node::Darkness(area.clone())]);
        let input = gather(&black, Audience::Tv, LOOK, VIEW, &Reveal::default()).unwrap();
        assert_eq!(input.zones.len(), 1);
        assert!(!input.fills);
        area.fill = crate::sight::Fill::Clouds;
        let clouds = scene(vec![Node::Darkness(area)]);
        assert!(
            gather(&clouds, Audience::Tv, LOOK, VIEW, &Reveal::default())
                .unwrap()
                .fills
        );
    }

    #[test]
    fn a_scene_with_no_light_draws_no_light() {
        let plain = scene(vec![walls(vec![Wall::new((0.0, 0.0), (1.0, 0.0))])]);
        assert_eq!(
            gather(&plain, Audience::Tv, LOOK, VIEW, &Reveal::default()),
            None
        );
    }

    #[test]
    fn a_light_far_off_the_view_counts_for_nothing_but_keeps_the_dark() {
        let far = scene(vec![Node::Light(Light::new(2, (100.0, 0.0), 1.0, 2.0))]);
        let input = gather(&far, Audience::Tv, LOOK, VIEW, &Reveal::default()).unwrap();
        assert_eq!(input.lights.len(), 0);
    }

    #[test]
    fn a_light_off_the_tv_lights_only_the_dm_screen() {
        let mut light = Light::new(2, (0.0, 0.0), 1.0, 2.0);
        light.shown.tv = false;
        let only_dm = scene(vec![Node::Light(light)]);
        assert_eq!(
            gather(&only_dm, Audience::Dm, LOOK, VIEW, &Reveal::default())
                .unwrap()
                .lights
                .len(),
            1
        );
        assert_eq!(
            gather(&only_dm, Audience::Tv, LOOK, VIEW, &Reveal::default()),
            None
        );
    }

    #[test]
    fn an_open_door_and_a_far_wall_cast_no_shadow() {
        let mut door = Wall::new((0.0, 1.0), (1.0, 1.0));
        door.kind = Kind::Door { open: true };
        let near = Wall::new((0.0, -1.0), (1.0, -1.0));
        let far = Wall::new((50.0, 0.0), (51.0, 0.0));
        let room = scene(vec![
            walls(vec![door, near, far]),
            Node::Light(Light::new(2, (0.0, 0.0), 1.0, 3.0)),
        ]);
        let input = gather(&room, Audience::Tv, LOOK, VIEW, &Reveal::default()).unwrap();
        assert_eq!(input.walls, vec![[0.0, -1.0, 1.0, -1.0]]);
        let closed = Rules {
            open_doors_pass: false,
            ..LOOK
        };
        assert_eq!(
            gather(&room, Audience::Tv, closed, VIEW, &Reveal::default())
                .unwrap()
                .walls
                .len(),
            2
        );
        let no_walls = Rules {
            walls_block: false,
            ..LOOK
        };
        let input = gather(&room, Audience::Tv, no_walls, VIEW, &Reveal::default()).unwrap();
        assert_eq!(input.walls.len(), 0);
    }

    #[test]
    fn each_light_reads_only_the_walls_it_reaches() {
        let left = Wall::new((-6.0, -1.0), (-6.0, 1.0));
        let right = Wall::new((6.0, -1.0), (6.0, 1.0));
        let room = scene(vec![
            walls(vec![left, right]),
            Node::Light(Light::new(2, (-5.0, 0.0), 1.0, 2.0)),
            Node::Light(Light::new(3, (5.0, 0.0), 1.0, 2.0)),
        ]);
        let input = gather(&room, Audience::Tv, LOOK, VIEW, &Reveal::default()).unwrap();
        assert_eq!(
            input.walls,
            vec![[-6.0, -1.0, -6.0, 1.0], [6.0, -1.0, 6.0, 1.0]]
        );
        let run = |light: &[f32; 12]| (light[10].to_bits(), light[11].to_bits());
        assert_eq!(run(&input.lights[0]), (0, 1));
        assert_eq!(run(&input.lights[1]), (1, 1));
    }

    #[test]
    fn an_eye_of_a_reveal_comes_after_the_lamps_and_meets_the_walls_of_sight() {
        let mut window = Wall::new((1.0, -1.0), (1.0, 1.0));
        window.blocks_sight = false;
        let mut door = Wall::new((-1.0, -1.0), (-1.0, 1.0));
        door.kind = Kind::Door { open: true };
        let room = scene(vec![
            Node::Light(Light::new(2, (0.0, 5.0), 1.0, 2.0)),
            walls(vec![Wall::new((0.0, -1.0), (0.0, 1.0)), window, door]),
        ]);
        let reveal = Reveal {
            eyes: vec![crate::ink::Eye {
                center: (0.0, 0.0),
                reach: 2.0,
            }],
            mesh: vec![([0.0, 0.0], 0); 3],
        };
        let input = gather(&room, Audience::Tv, LOOK, VIEW, &reveal).unwrap();
        assert_eq!((input.lamps, input.lights.len()), (1, 2));
        // The triangles name the row after the one lamp.
        assert!(input.reveal.iter().all(|spot| spot[2].to_bits() == 1));
        // The eye meets the plain wall, and not the window or the open door.
        let eye = input.lights[1];
        let (first, count) = (eye[10].to_bits() as usize, eye[11].to_bits() as usize);
        assert_eq!(count, 1);
        assert_eq!(input.walls[first], [0.0, -1.0, 0.0, 1.0]);
    }

    #[test]
    fn a_reveal_alone_still_darkens_a_scene_with_no_light() {
        let reveal = Reveal {
            eyes: vec![crate::ink::Eye {
                center: (0.0, 0.0),
                reach: 1.0,
            }],
            mesh: Vec::new(),
        };
        let plain = scene(Vec::new());
        let input = gather(&plain, Audience::Tv, LOOK, VIEW, &reveal).unwrap();
        assert_eq!((input.lamps, input.lights.len()), (0, 1));
        assert!(!input.plain);
    }

    #[test]
    fn a_conceal_alone_draws_and_leaves_the_rest_of_the_map_as_it_is() {
        let conceal = Reveal {
            eyes: Vec::new(),
            mesh: vec![([0.0, 0.0], crate::ink::CONCEAL); 3],
        };
        let plain = scene(Vec::new());
        let input = gather(&plain, Audience::Tv, LOOK, VIEW, &conceal).unwrap();
        assert!(input.plain);
        assert_eq!(input.lights.len(), 0);
        assert!(input.reveal.iter().all(|spot| spot[3].to_bits() == 1));
    }
}
