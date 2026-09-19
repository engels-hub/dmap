//! The dialog that finds the grid on a map file. DESIGN.md 9.9.
//!
//! The search runs on a thread of its own and hands back every straight
//! line it found, with a small copy of the image to show them on. The DM
//! picks two lines that bound a cell, or two lines some cells apart, and
//! the dialog writes the size of a cell to the map. Nothing here guesses:
//! a tile with a joint across its middle gives a line there too, and only
//! the DM can tell it from the grid. Issue #35.

// Rust guideline compliant 2026-02-21

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use crate::command::SetAssets;
use crate::grid::Cells;
use crate::images;
use crate::lines::{self, Line, Way};
use crate::scene::{Node, NodeId};
use crate::text;
use crate::theme::{self, Tokens};
use crate::transform::{MAX_GRID_PX, MIN_GRID_PX};
use crate::widget::{self, Height};

use super::dialog::dialog_frame;
use super::{FOOTER, Frame};

/// The size of the dialog, in points.
///
/// A map is the whole point of it, so it takes more room than the 740 of
/// DESIGN.md 9. The dialog frame shrinks it to the window when it must.
const SIZE: egui::Vec2 = egui::vec2(1040.0, 760.0);

/// The longest side of the copy of the image the dialog shows, in pixels.
///
/// The lines are drawn from where the search found them, not from the
/// copy, so a small copy costs the DM nothing in precision. The copy never
/// outgrows the largest texture `egui` takes either, which on some GPUs is
/// half of this. A larger one ends the program.
const PREVIEW_SIDE: u32 = 4096;

/// How close the pointer must come to a line to take it, in points.
///
/// Six is as close as a hand lands on a thin line without a second try,
/// and far enough apart that two lines of a small cell stay two targets
/// once the DM zooms in.
const REACH: f32 = 6.0;

/// How much a notch of the wheel zooms, per point of scroll.
const ZOOM_PER_POINT: f64 = 0.002;

/// The closest the dialog zooms in, in points for one pixel of the map.
///
/// At forty a line of one pixel stands forty points wide, which is past
/// any need to tell two lines apart.
const CLOSEST: f64 = 40.0;

/// The furthest the dialog zooms out, as a share of the zoom that fits.
const FURTHEST: f64 = 0.25;

/// The most cells the DM may say lie between the two lines.
const MOST_CELLS: f64 = 1000.0;

/// What the search thread sends back.
struct Found {
    /// The size of the file, in pixels. The lines count in these.
    size: (u32, u32),
    lines: Vec<Line>,
    /// A copy of the image no larger than [`PREVIEW_SIDE`] on a side.
    preview: egui::ColorImage,
}

/// What the dialog shows once the search is back.
struct Seen {
    size: (u32, u32),
    lines: Vec<Line>,
    texture: egui::TextureHandle,
}

/// Where the dialog looks at the image.
#[derive(Debug, Clone, Copy)]
struct Look {
    /// The pixel of the file in the middle of the view.
    center: (f64, f64),
    /// Points for one pixel of the file.
    zoom: f64,
    /// The zoom that fits the whole image in the view.
    fit: f64,
}

impl Look {
    /// A look that holds the whole image, in the middle of `view`.
    fn fitting(size: (u32, u32), view: egui::Rect) -> Self {
        let fit = (f64::from(view.width()) / f64::from(size.0.max(1)))
            .min(f64::from(view.height()) / f64::from(size.1.max(1)));
        Self {
            center: (f64::from(size.0) / 2.0, f64::from(size.1) / 2.0),
            zoom: fit,
            fit,
        }
    }

    /// Where a pixel of the file stands on the screen.
    fn screen(self, view: egui::Rect, pixel: (f64, f64)) -> egui::Pos2 {
        egui::pos2(
            view.center().x + ((pixel.0 - self.center.0) * self.zoom) as f32,
            view.center().y + ((pixel.1 - self.center.1) * self.zoom) as f32,
        )
    }

    /// The pixel of the file under a point of the screen.
    fn pixel(self, view: egui::Rect, point: egui::Pos2) -> (f64, f64) {
        (
            self.center.0 + f64::from(point.x - view.center().x) / self.zoom,
            self.center.1 + f64::from(point.y - view.center().y) / self.zoom,
        )
    }

    /// The same look, zoomed by `factor` around the point under `point`.
    fn zoomed(self, view: egui::Rect, point: egui::Pos2, factor: f64) -> Self {
        let held = self.pixel(view, point);
        let zoom = (self.zoom * factor).clamp(self.fit * FURTHEST, CLOSEST.max(self.fit));
        Self {
            center: (
                held.0 - f64::from(point.x - view.center().x) / zoom,
                held.1 - f64::from(point.y - view.center().y) / zoom,
            ),
            zoom,
            fit: self.fit,
        }
    }
}

/// The dialog's state between frames.
#[derive(Default)]
pub(super) struct Finder {
    /// The map the dialog reads, or `None` while it stands closed.
    map: Option<NodeId>,
    /// The search that has not come back yet.
    search: Option<Receiver<Result<Found, String>>>,
    /// What the search gave, or what went wrong.
    seen: Option<Result<Seen, String>>,
    look: Option<Look>,
    /// The lines the DM picked, as places in the list of lines. Two at
    /// most, and both run the same way.
    picked: Vec<usize>,
    /// How many cells lie between the two lines.
    cells: f64,
}

impl std::fmt::Debug for Finder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Finder")
            .field("map", &self.map)
            .field("picked", &self.picked)
            .finish_non_exhaustive()
    }
}

impl Finder {
    /// Whether the dialog stands over the canvas.
    pub(super) fn is_open(&self) -> bool {
        self.map.is_some()
    }

    /// Opens the dialog on a map, and starts the search on its file.
    ///
    /// The search reads the file again, at its whole size, because the
    /// copy on the GPU may be a smaller one and a cell wants every pixel.
    pub(super) fn open(&mut self, ctx: &egui::Context, id: NodeId, file: PathBuf) {
        let (sender, receiver) = channel();
        let wake = ctx.clone();
        let side = ctx
            .input(|input| input.max_texture_side)
            .try_into()
            .map_or(PREVIEW_SIDE, |largest: u32| largest.min(PREVIEW_SIDE));
        std::thread::spawn(move || {
            // A dialog closed before the search ends drops the receiver,
            // and nothing waits for the answer.
            let _ = sender.send(search(&file, side));
            wake.request_repaint();
        });
        *self = Self {
            map: Some(id),
            search: Some(receiver),
            cells: 1.0,
            ..Self::default()
        };
    }

    /// Takes what the search sent back, when it sent anything.
    ///
    /// A search thread that stops without an answer drops its end of the
    /// channel. The dialog says so. Without this it would say that it reads
    /// the map for as long as it stands.
    fn poll(&mut self, ctx: &egui::Context) {
        let Some(receiver) = &self.search else {
            return;
        };
        let seen = match receiver.try_recv() {
            Ok(result) => result.map(|found| Seen {
                size: found.size,
                lines: found.lines,
                texture: ctx.load_texture("finder", found.preview, egui::TextureOptions::LINEAR),
            }),
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err(text::dialog_finder_failed().to_owned()),
        };
        self.search = None;
        self.seen = Some(seen);
    }
}

/// Reads a map file and finds its lines. Runs on the search thread.
///
/// `longest` is the longest side the copy for the dialog may take.
fn search(file: &Path, longest: u32) -> Result<Found, String> {
    let image = images::open(file)?;
    let size = (image.width(), image.height());
    let lines = {
        let gray = image.to_luma8();
        lines::find(gray.as_raw(), size)
    };
    let small = if size.0.max(size.1) > longest {
        image.thumbnail(longest, longest)
    } else {
        image
    }
    .to_rgba8();
    let preview = egui::ColorImage::from_rgba_unmultiplied(
        [small.width() as usize, small.height() as usize],
        small.as_raw(),
    );
    Ok(Found {
        size,
        lines,
        preview,
    })
}

/// The Find the grid dialog. Returns `true` when the scene changed.
fn finder_dialog(
    ui: &egui::Ui,
    finder: &mut Finder,
    frame: &mut Frame<'_>,
    tokens: Tokens,
) -> bool {
    let Some(id) = finder.map else {
        return false;
    };
    // A map that left the scene, by an undo or a delete, takes the dialog
    // with it.
    let Some(map) = crate::scene::find(frame.scene, id)
        .and_then(Node::asset)
        .cloned()
    else {
        *finder = Finder::default();
        return false;
    };
    let ctx = ui.ctx().clone();
    finder.poll(&ctx);
    // Use writes the size in the pixels of the copy on the GPU, so it
    // waits until the loader has made that copy. Issue #35.
    let gpu = (frame.size_of)(&map.path);
    let cells = frame.settings.cells();
    let mut chosen: Option<(Way, f64, f64)> = None;
    let mut wants_use = false;
    let title = text::dialog_finder_title();
    let close = dialog_frame(&ctx, "finder", title, SIZE, tokens, |ui, rest| {
        let footer = egui::Rect::from_min_size(
            egui::pos2(rest.left(), rest.bottom() - FOOTER),
            egui::vec2(rest.width(), FOOTER),
        );
        let view = egui::Rect::from_min_max(
            egui::pos2(rest.left() + 20.0, rest.top() + 18.0),
            egui::pos2(rest.right() - 20.0, footer.top() - 18.0),
        );
        ui.painter().rect_filled(view, 0, tokens.field);
        if let Some(Ok(seen)) = &finder.seen {
            let look = finder
                .look
                .get_or_insert_with(|| Look::fitting(seen.size, view));
            let hovered = image_view(ui, view, seen, look, &finder.picked, finder.cells, tokens);
            if let Some(index) = hovered.filter(|_| ui.input(|i| !i.key_down(egui::Key::Space)))
                && ui.input(|i| i.pointer.primary_clicked())
            {
                pick(&mut finder.picked, &seen.lines, index);
            }
            if let [first, second] = finder.picked[..] {
                let (first, second) = (seen.lines[first], seen.lines[second]);
                chosen = Some((first.way, first.at, second.at));
            }
        }
        wants_use = footer_row(
            ui,
            footer,
            finder,
            chosen,
            (gpu.is_some(), cells.kind.is_hex()),
            tokens,
        );
    });
    let mut edited = false;
    if wants_use
        && let (Some((way, first, second)), Some(Ok(seen))) = (chosen, &finder.seen)
        && let Some(cell) = cell_of(first, second, finder.cells)
        && let Some(gpu) = gpu
    {
        let after = grid_of(&map, seen.size, gpu, cells, way, first, cell);
        frame.history.run(
            frame.scene,
            SetAssets {
                before: vec![map],
                after: vec![after],
            },
        );
        edited = true;
        finder.map = None;
    }
    if close || finder.map.is_none() {
        *finder = Finder::default();
    }
    edited
}

/// The footer: Cells between, what the dialog has to say, Cancel and Use.
///
/// `(loaded, hex)` says whether the map has its copy on the GPU, and whether
/// the canvas grid is a hex grid. Use stands dead until the copy is there.
/// Returns `true` when the DM pressed Use. Cancel closes the dialog here.
fn footer_row(
    ui: &mut egui::Ui,
    footer: egui::Rect,
    finder: &mut Finder,
    chosen: Option<(Way, f64, f64)>,
    (loaded, hex): (bool, bool),
    tokens: Tokens,
) -> bool {
    widget::rule_bottom(
        ui,
        egui::Rect::from_min_size(footer.left_top(), egui::vec2(footer.width(), 0.0)),
        tokens,
    );
    let mut foot = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(footer.shrink2(egui::vec2(20.0, 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    foot.spacing_mut().item_spacing.x = 10.0;
    foot.label(
        egui::RichText::new(text::dialog_finder_cells())
            .font(theme::font(theme::BODY, false))
            .color(tokens.ink),
    );
    if widget::input(
        &mut foot,
        &mut finder.cells,
        "",
        64.0,
        1.0..=MOST_CELLS,
        // A whole step, so the field shows a count and not a fraction.
        1.0,
    )
    .changed()
    {
        finder.cells = finder.cells.round().max(1.0);
    }
    let cell = chosen.and_then(|(_, first, second)| cell_of(first, second, finder.cells));
    let says = say(finder, cell, loaded, hex);
    let mut wants_use = false;
    // DESIGN.md 9: the last button sits on the right.
    foot.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        wants_use = ui
            .add_enabled_ui(cell.is_some() && loaded, |ui| {
                widget::button(ui, text::dialog_finder_use(), None, Height::Full).clicked()
            })
            .inner;
        if widget::button(ui, text::dialog_finder_cancel(), None, Height::Full).clicked() {
            finder.map = None;
        }
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            widget::helper(ui, &says);
        });
    });
    wants_use
}

/// Opens the dialog when the Map panel asked for it, and draws it.
///
/// Returns `true` when the scene changed.
pub(super) fn find_the_grid(
    ui: &egui::Ui,
    finder: &mut Finder,
    asked: Option<NodeId>,
    frame: &mut Frame<'_>,
    tokens: Tokens,
) -> bool {
    if let Some(id) = asked
        && let Some(map) = crate::scene::find(frame.scene, id).and_then(Node::asset)
    {
        finder.open(ui.ctx(), id, frame.scene_dir.join(&map.path));
    }
    finder_dialog(ui, finder, frame, tokens)
}

/// Draws the image and its lines, and moves the look with the hand.
///
/// Returns the line under the pointer, as a place in the list of lines.
fn image_view(
    ui: &egui::Ui,
    view: egui::Rect,
    seen: &Seen,
    look: &mut Look,
    picked: &[usize],
    cells: f64,
    tokens: Tokens,
) -> Option<usize> {
    let response = ui.interact(
        view,
        ui.id().with("finder view"),
        egui::Sense::click_and_drag(),
    );
    let space = ui.input(|i| i.key_down(egui::Key::Space));
    // The middle button pans, and so does `Space` with the first button,
    // as the canvas does. DESIGN.md 5.1.
    if response.dragged_by(egui::PointerButton::Middle)
        || (space && response.dragged_by(egui::PointerButton::Primary))
    {
        let delta = response.drag_delta();
        look.center.0 -= f64::from(delta.x) / look.zoom;
        look.center.1 -= f64::from(delta.y) / look.zoom;
    }
    if let (true, Some(point)) = (response.hovered(), response.hover_pos()) {
        // The wheel zooms here, and so does a pinch or `Ctrl` with the
        // wheel, which egui hands over as a zoom of its own.
        let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
        let factor = f64::from(pinch) * (f64::from(scroll) * ZOOM_PER_POINT).exp();
        if (factor - 1.0).abs() > f64::EPSILON {
            *look = look.zoomed(view, point, factor);
        }
    }
    let painter = ui.painter_at(view);
    let image = egui::Rect::from_two_pos(
        look.screen(view, (0.0, 0.0)),
        look.screen(view, (f64::from(seen.size.0), f64::from(seen.size.1))),
    );
    painter.image(
        seen.texture.id(),
        image,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
    let hovered = response
        .hover_pos()
        .filter(|point| image.contains(*point))
        .and_then(|point| nearest(seen, *look, view, point));
    for (index, line) in seen.lines.iter().enumerate() {
        if picked.contains(&index) || hovered == Some(index) {
            continue;
        }
        // A line reads on a dark map and on a pale one when a pale stroke
        // lies on a dark one. A faint line draws faint, so the grid stands
        // out of the pattern inside its tiles.
        let solid = 0.35 + 0.65 * line.strength;
        let ends = ends_of(*line, *look, view, image);
        painter.line_segment(
            ends,
            egui::Stroke::new(3.0, egui::Color32::BLACK.gamma_multiply(0.45 * solid)),
        );
        painter.line_segment(
            ends,
            egui::Stroke::new(1.0, egui::Color32::WHITE.gamma_multiply(solid)),
        );
    }
    if let [first, second] = picked[..]
        && let Some(cell) = cell_of(seen.lines[first].at, seen.lines[second].at, cells)
    {
        preview(
            &painter,
            seen,
            *look,
            view,
            image,
            seen.lines[first],
            cell,
            tokens,
        );
    }
    if let Some(index) = hovered {
        let ends = ends_of(seen.lines[index], *look, view, image);
        painter.line_segment(ends, egui::Stroke::new(2.0, tokens.accent));
    }
    for &index in picked {
        let ends = ends_of(seen.lines[index], *look, view, image);
        painter.line_segment(ends, egui::Stroke::new(3.0, tokens.accent));
    }
    if hovered.is_some() && !space {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    hovered
}

/// The grid the two lines give, drawn over the whole image.
///
/// The DM sees at once whether it holds to the far edge of the map, or
/// drifts off the lines of the map as it goes.
#[expect(
    clippy::too_many_arguments,
    reason = "one drawing: where it goes, what it reads, and in which color"
)]
fn preview(
    painter: &egui::Painter,
    seen: &Seen,
    look: Look,
    view: egui::Rect,
    image: egui::Rect,
    first: Line,
    cell: f64,
    tokens: Tokens,
) {
    let length = match first.way {
        Way::Down => f64::from(seen.size.0),
        Way::Across => f64::from(seen.size.1),
    };
    let start = first.at.rem_euclid(cell);
    let count = ((length - start) / cell).floor() as usize;
    let stroke = egui::Stroke::new(1.0, tokens.accent.gamma_multiply(0.7));
    for step in 0..=count {
        let at = start + cell * step as f64;
        let line = Line { at, ..first };
        painter.line_segment(ends_of(line, look, view, image), stroke);
    }
}

/// The two ends of a line on the screen, from one edge of the image to
/// the other.
fn ends_of(line: Line, look: Look, view: egui::Rect, image: egui::Rect) -> [egui::Pos2; 2] {
    match line.way {
        Way::Down => {
            let x = look.screen(view, (line.at, 0.0)).x;
            [egui::pos2(x, image.top()), egui::pos2(x, image.bottom())]
        }
        Way::Across => {
            let y = look.screen(view, (0.0, line.at)).y;
            [egui::pos2(image.left(), y), egui::pos2(image.right(), y)]
        }
    }
}

/// The line closest to `point`, when one lies within [`REACH`].
fn nearest(seen: &Seen, look: Look, view: egui::Rect, point: egui::Pos2) -> Option<usize> {
    let pixel = look.pixel(view, point);
    seen.lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let off = match line.way {
                Way::Down => pixel.0 - line.at,
                Way::Across => pixel.1 - line.at,
            };
            (index, (off.abs() * look.zoom) as f32)
        })
        .filter(|(_, points)| *points <= REACH)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(index, _)| index)
}

/// Takes a click on a line into what the DM picked.
///
/// A click on a picked line lets it go. A line that runs the other way
/// starts the pick over, because two lines across one another bound no
/// cell. A third line of the same way takes the place of the second.
fn pick(picked: &mut Vec<usize>, lines: &[Line], index: usize) {
    if let Some(place) = picked.iter().position(|&held| held == index) {
        picked.remove(place);
        return;
    }
    let way = lines[index].way;
    if picked.first().is_some_and(|&held| lines[held].way != way) {
        picked.clear();
    }
    if picked.len() == 2 {
        picked[1] = index;
    } else {
        picked.push(index);
    }
}

/// The pixels of one cell, when the two lines and the count give one.
fn cell_of(first: f64, second: f64, cells: f64) -> Option<f64> {
    let cells = u32::try_from(cells.round() as i64).ok()?;
    lines::cell_size(first, second, cells).filter(|cell| (MIN_GRID_PX..=MAX_GRID_PX).contains(cell))
}

/// The helper line of the footer: what the dialog waits for, or the size.
///
/// `loaded` says whether the map has its copy on the GPU, which Use waits
/// for. `hex` says whether the canvas grid is a hex grid.
fn say(finder: &Finder, cell: Option<f64>, loaded: bool, hex: bool) -> String {
    match (&finder.seen, cell) {
        (None, _) => text::dialog_finder_searching().to_owned(),
        (Some(Err(error)), _) => error.clone(),
        (Some(Ok(seen)), _) if seen.lines.is_empty() => text::dialog_finder_none().to_owned(),
        (Some(Ok(_)), Some(cell)) => {
            let size = text::dialog_finder_result(format_args!("{cell:.2}"));
            if loaded {
                size
            } else {
                format!("{size}. {}", text::dialog_finder_loading())
            }
        }
        // A hex grid gives a line at each half hex, and two neighbors bound
        // no cell. The search reads no hex grid, so the helper says how to
        // pick one by hand.
        (Some(Ok(_)), None) if hex => text::dialog_finder_hex().to_owned(),
        (Some(Ok(_)), None) => text::dialog_finder_pick().to_owned(),
    }
}

/// The map with the size of a cell the DM picked, and where its grid starts.
///
/// `file` is the size of the file, and `gpu` the size of the copy the canvas
/// draws, both in pixels. The size goes into the pixels of that copy: a map
/// past the largest texture draws from a smaller copy and counts its cells
/// on it. The start goes into the snap offset of #32, on the axis the two
/// lines give, so a snapped move lays the map on the canvas grid.
///
/// Two maps keep the offset they had. A map at an angle has axes that are
/// not the axes of the canvas. A hex canvas snaps to the middle of a hex,
/// not to a line, so an offset from a line puts the map half a hex off.
fn grid_of(
    map: &crate::scene::Asset,
    file: (u32, u32),
    gpu: (u32, u32),
    cells: Cells,
    way: Way,
    first: f64,
    cell: f64,
) -> crate::scene::Asset {
    let on_gpu = f64::from(gpu.0) / f64::from(file.0.max(1));
    let mut after = map.clone();
    after.grid_px = (cell * on_gpu).clamp(MIN_GRID_PX, MAX_GRID_PX);
    let turned = map.rotation.rem_euclid(std::f64::consts::TAU);
    let square = turned < 1e-9 || std::f64::consts::TAU - turned < 1e-9;
    if !square || cells.kind.is_hex() {
        return after;
    }
    let step = cells.width();
    match way {
        Way::Down => {
            let at = if map.flip_x {
                f64::from(file.0) - first
            } else {
                first
            };
            after.snap_offset.0 = lines::offset(at, cell, map.scale, step);
        }
        Way::Across => {
            let at = if map.flip_y {
                f64::from(file.1) - first
            } else {
                first
            };
            after.snap_offset.1 = lines::offset(at, cell, map.scale, step);
        }
    }
    after
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::mpsc::channel;

    use super::{Finder, Found, Look, Seen, grid_of, pick, say};
    use crate::grid::{Cells, Kind};
    use crate::lines::{self, Line, Way};
    use crate::scene::{Asset, Shown};
    use crate::text;

    /// A map at true size, one inch to fifty pixels, with an offset of its own.
    fn map() -> Asset {
        Asset {
            id: 1,
            shown: Shown::default(),
            path: PathBuf::from("map.png"),
            center: (0.0, 0.0),
            grid_px: 50.0,
            rotation: 0.0,
            scale: 1.0,
            flip_x: false,
            flip_y: false,
            snap_offset: (0.3, 0.2),
        }
    }

    /// What the search gives for an image of 400 by 300 with these lines.
    fn seen(lines: Vec<Line>) -> Seen {
        let ctx = egui::Context::default();
        let image = egui::ColorImage::from_rgba_unmultiplied([1, 1], &[0, 0, 0, 255]);
        Seen {
            size: (400, 300),
            lines,
            texture: ctx.load_texture("test", image, egui::TextureOptions::LINEAR),
        }
    }

    #[test]
    fn a_search_that_stops_without_an_answer_says_so() {
        let ctx = egui::Context::default();
        let (sender, receiver) = channel::<Result<Found, String>>();
        let mut finder = Finder {
            map: Some(1),
            search: Some(receiver),
            cells: 1.0,
            ..Finder::default()
        };
        // A search at work sends nothing yet, and the dialog waits.
        finder.poll(&ctx);
        assert!(finder.search.is_some() && finder.seen.is_none());
        // A thread that ends without a send drops its end of the channel.
        drop(sender);
        finder.poll(&ctx);
        assert!(finder.search.is_none());
        assert!(
            matches!(&finder.seen, Some(Err(error)) if error == text::dialog_finder_failed()),
            "the dialog must stop saying that it reads the map"
        );
    }

    #[test]
    fn use_waits_for_the_copy_on_the_gpu_and_says_why() {
        let finder = Finder {
            seen: Some(Ok(seen(vec![line(Way::Down, 10.0)]))),
            ..Finder::default()
        };
        let size = text::dialog_finder_result(format_args!("40.00"));
        assert_eq!(say(&finder, Some(40.0), true, false), size);
        let waits = say(&finder, Some(40.0), false, false);
        assert!(waits.starts_with(&size), "the size still shows: {waits}");
        assert!(waits.ends_with(text::dialog_finder_loading()), "{waits}");
    }

    #[test]
    fn a_hex_canvas_says_how_to_pick_a_hex() {
        let finder = Finder {
            seen: Some(Ok(seen(vec![line(Way::Down, 10.0)]))),
            ..Finder::default()
        };
        assert_eq!(say(&finder, None, true, true), text::dialog_finder_hex());
        assert_eq!(say(&finder, None, true, false), text::dialog_finder_pick());
    }

    #[test]
    fn use_writes_the_size_in_the_pixels_of_the_copy() {
        // A file of 12000 pixels draws from a copy of 6000, so a cell of 140
        // pixels of the file is 70 of the copy.
        let after = grid_of(
            &map(),
            (12000, 6000),
            (6000, 3000),
            Cells::default(),
            Way::Down,
            10.0,
            140.0,
        );
        assert!((after.grid_px - 70.0).abs() < 1e-9, "{}", after.grid_px);
    }

    #[test]
    fn a_square_canvas_takes_the_start_of_the_grid() {
        let after = grid_of(
            &map(),
            (400, 300),
            (400, 300),
            Cells::default(),
            Way::Down,
            10.0,
            40.0,
        );
        assert!((after.snap_offset.0 - lines::offset(10.0, 40.0, 1.0, 1.0)).abs() < 1e-9);
        // The two lines run down, so they say nothing of the other axis.
        assert!((after.snap_offset.1 - 0.2).abs() < 1e-9);
    }

    #[test]
    fn a_hex_canvas_keeps_the_offset_the_map_had() {
        // A hex canvas snaps to the middle of a hex. An offset from a line
        // would put the map half a hex off, so the map keeps its own.
        for kind in [Kind::HexPointyTop, Kind::HexFlatTop] {
            let cells = Cells { kind, cell: 1.0 };
            let after = grid_of(&map(), (400, 300), (400, 300), cells, Way::Down, 10.0, 40.0);
            assert_eq!(after.snap_offset, map().snap_offset, "{kind:?}");
            // The size is still the size.
            assert!((after.grid_px - 40.0).abs() < 1e-9);
        }
    }

    fn line(way: Way, at: f64) -> Line {
        Line {
            way,
            at,
            strength: 1.0,
        }
    }

    #[test]
    fn a_click_picks_a_line_and_a_second_click_lets_it_go() {
        let lines = [line(Way::Down, 10.0), line(Way::Down, 50.0)];
        let mut picked = Vec::new();
        pick(&mut picked, &lines, 0);
        assert_eq!(picked, vec![0]);
        pick(&mut picked, &lines, 1);
        assert_eq!(picked, vec![0, 1]);
        pick(&mut picked, &lines, 0);
        assert_eq!(picked, vec![1]);
    }

    #[test]
    fn a_line_across_the_first_starts_the_pick_over() {
        let lines = [
            line(Way::Down, 10.0),
            line(Way::Down, 50.0),
            line(Way::Across, 30.0),
        ];
        let mut picked = vec![0, 1];
        pick(&mut picked, &lines, 2);
        assert_eq!(picked, vec![2]);
    }

    #[test]
    fn a_third_line_takes_the_place_of_the_second() {
        let lines = [
            line(Way::Down, 10.0),
            line(Way::Down, 50.0),
            line(Way::Down, 90.0),
        ];
        let mut picked = vec![0, 1];
        pick(&mut picked, &lines, 2);
        assert_eq!(picked, vec![0, 2]);
    }

    #[test]
    fn a_zoom_keeps_the_pixel_under_the_pointer() {
        let view = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 300.0));
        let look = Look::fitting((2000, 1000), view);
        let point = egui::pos2(120.0, 90.0);
        let before = look.pixel(view, point);
        let zoomed = look.zoomed(view, point, 3.0);
        let after = zoomed.pixel(view, point);
        assert!((before.0 - after.0).abs() < 1e-6 && (before.1 - after.1).abs() < 1e-6);
        assert!((zoomed.zoom - look.zoom * 3.0).abs() < 1e-9);
    }

    #[test]
    fn the_fit_holds_the_whole_image() {
        let view = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 300.0));
        let look = Look::fitting((2000, 1000), view);
        let corner = look.screen(view, (2000.0, 1000.0));
        assert!(
            corner.x <= 400.0 + 1e-3 && corner.y <= 300.0 + 1e-3,
            "{corner:?}"
        );
    }
}
