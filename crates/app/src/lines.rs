//! The straight lines of a map image, for the DM to pick a grid from.
//!
//! A grid line runs down or across the whole map, so it darkens or lights
//! one column or one row of pixels on the mean. The search reads that from
//! the mean of each column and each row, and it picks nothing: it hands
//! every line it finds to the dialog, and the DM picks the two that bound
//! a cell. A map with a finer pattern inside each tile fools a program
//! that guesses, and it does not fool a DM who can see the map. Issue #35.

// Rust guideline compliant 2026-02-21

/// Which way a line runs on the image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Way {
    /// Top to bottom. The line stands at one column.
    Down,
    /// Left to right. The line stands at one row.
    Across,
}

/// A straight line the search found.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line {
    /// Which way the line runs.
    pub way: Way,
    /// Where the middle of the line stands, in pixels.
    ///
    /// A line that runs down counts from the left edge, and a line that
    /// runs across counts from the top. The count reaches between two
    /// whole pixels, so a cell read over many lines stays exact.
    pub at: f64,
    /// How much of the image the line runs along, from 0 to 1.
    ///
    /// A grid line crosses the map, and a wall on the map crosses a part
    /// of it. The dialog draws a strong line more solid than a faint one.
    pub strength: f32,
}

/// The widest half of a line the search reads, in pixels.
///
/// A position is compared with the positions this far to each side, for
/// each distance from one up. Four reads a line up to eight pixels thick,
/// which covers the grid of every VTT map in sight. A wider reach would
/// merge two lines of a small cell into one.
const WIDEST: usize = 4;

/// How many times the noise a line must stand over it.
///
/// Six keeps the grain of a painted floor out: a column of noise reaches
/// six times its median about once in a very large image.
const NOISE_TIMES: f64 = 6.0;

/// The least noise the search assumes, in levels of gray on the mean.
///
/// A clean drawn image has no noise at all, and without a floor every
/// column that differs by a hair would count as a line.
const NOISE_FLOOR: f64 = 0.5;

/// How far a pixel must stand from its sides to count as on the line.
///
/// Six levels of 255 is a step the eye sees on a dark map and on a pale
/// one. It decides the strength of a line and never whether it is one.
const PIXEL_STEP: f64 = 6.0;

/// The most lines the search gives back in each way.
///
/// A map of four hundred columns of cells is past any table, and the
/// dialog must draw every line on every frame.
const MOST: usize = 400;

/// The narrowest cell two picked lines may give, in pixels.
///
/// Two lines closer than this are one line read twice.
const NARROWEST_CELL: f64 = 2.0;

/// Every straight line that runs down or across a gray image.
///
/// `gray` holds one byte for each pixel, row after row, top row first.
/// The lines come back in two runs, those that run down and then those
/// that run across, each from the left or the top.
pub fn find(gray: &[u8], size: (u32, u32)) -> Vec<Line> {
    let (width, height) = (size.0 as usize, size.1 as usize);
    if width == 0 || height == 0 || gray.len() < width * height {
        return Vec::new();
    }
    // One pass over the rows reads both means. A pass down each column
    // would jump a whole row of memory for each pixel it takes.
    let mut columns = vec![0_u64; width];
    let mut rows = vec![0_u64; height];
    for (y, row) in gray.chunks_exact(width).take(height).enumerate() {
        for (x, &value) in row.iter().enumerate() {
            columns[x] += u64::from(value);
            rows[y] += u64::from(value);
        }
    }
    let columns: Vec<f64> = columns
        .into_iter()
        .map(|sum| sum as f64 / height as f64)
        .collect();
    let rows: Vec<f64> = rows
        .into_iter()
        .map(|sum| sum as f64 / width as f64)
        .collect();
    let mut lines = search(Way::Down, &columns, height, |at, along| {
        gray[along * width + at]
    });
    lines.extend(search(Way::Across, &rows, width, |at, along| {
        gray[at * width + along]
    }));
    lines
}

/// The lines of one way, from the mean of each position.
///
/// `depth` is how many pixels a position holds, and `pixel(at, along)`
/// reads one of them.
fn search(way: Way, mean: &[f64], depth: usize, pixel: impl Fn(usize, usize) -> u8) -> Vec<Line> {
    let reply = responses(mean);
    let noise = median_size(&reply).max(NOISE_FLOOR);
    let limit = NOISE_TIMES * noise;
    let mut found: Vec<(f64, Line)> = (0..reply.len())
        .filter(|&at| reply[at].0.abs() > limit && tops(&reply, at))
        .map(|at| {
            let (value, reach) = reply[at];
            let line = Line {
                way,
                at: at as f64 + 0.5 + between(&reply, at),
                strength: along(value.signum(), at, reach, depth, &pixel),
            };
            (value.abs(), line)
        })
        .collect();
    // The strongest lines stay when there are too many to draw.
    found.sort_by(|a, b| b.0.total_cmp(&a.0));
    found.truncate(MOST);
    let mut lines: Vec<Line> = found.into_iter().map(|(_, line)| line).collect();
    lines.sort_by(|a, b| a.at.total_cmp(&b.at));
    lines
}

/// How far each position stands from the positions around it.
///
/// The value is signed, dark below zero and light above it, so a grain
/// that goes both ways cancels on the mean and a line does not. Each
/// position takes the distance, from one to [`WIDEST`], that gives the
/// largest step, and so a thick line answers at its middle.
fn responses(mean: &[f64]) -> Vec<(f64, usize)> {
    (0..mean.len())
        .map(|at| {
            let mut best = (0.0_f64, 1);
            for reach in 1..=WIDEST {
                if at < reach || at + reach >= mean.len() {
                    break;
                }
                let step = mean[at] - f64::midpoint(mean[at - reach], mean[at + reach]);
                if step.abs() > best.0.abs() {
                    best = (step, reach);
                }
            }
            best
        })
        .collect()
}

/// The median of the sizes of the responses, which is the noise.
fn median_size(reply: &[(f64, usize)]) -> f64 {
    let mut sizes: Vec<f64> = reply.iter().map(|(value, _)| value.abs()).collect();
    if sizes.is_empty() {
        return 0.0;
    }
    let middle = sizes.len() / 2;
    sizes.select_nth_unstable_by(middle, f64::total_cmp);
    sizes[middle]
}

/// Whether the response at `at` is the largest near it.
///
/// Two positions of one thick line answer alike, and the first of them
/// takes the line, so it comes back once.
fn tops(reply: &[(f64, usize)], at: usize) -> bool {
    let size = reply[at].0.abs();
    let near = WIDEST + 1;
    let from = at.saturating_sub(near);
    let to = (at + near).min(reply.len() - 1);
    (from..=to).all(|other| {
        let other_size = reply[other].0.abs();
        other == at || other_size < size || (other_size <= size && other > at)
    })
}

/// How far between two whole pixels the middle of a line stands.
///
/// A parabola through the peak and its two sides puts the top where the
/// three values lean. Takes a value from -0.5 to 0.5.
fn between(reply: &[(f64, usize)], at: usize) -> f64 {
    if at == 0 || at + 1 >= reply.len() {
        return 0.0;
    }
    let (left, middle, right) = (
        reply[at - 1].0.abs(),
        reply[at].0.abs(),
        reply[at + 1].0.abs(),
    );
    let bend = left - 2.0 * middle + right;
    if bend >= 0.0 {
        return 0.0;
    }
    (0.5 * (left - right) / bend).clamp(-0.5, 0.5)
}

/// How much of the image the line at `at` runs along, from 0 to 1.
fn along(
    sign: f64,
    at: usize,
    reach: usize,
    depth: usize,
    pixel: &impl Fn(usize, usize) -> u8,
) -> f32 {
    if depth == 0 || at < reach {
        return 0.0;
    }
    let on = (0..depth)
        .filter(|&step| {
            let side = f64::midpoint(
                f64::from(pixel(at - reach, step)),
                f64::from(pixel(at + reach, step)),
            );
            sign * (f64::from(pixel(at, step)) - side) > PIXEL_STEP
        })
        .count();
    (on as f64 / depth as f64) as f32
}

/// The pixels in one cell, from two lines `cells` cells apart.
///
/// Returns `None` for no cells between, or for two lines that stand on
/// one another.
pub fn cell_size(first: f64, second: f64, cells: u32) -> Option<f64> {
    let gap = (second - first).abs();
    if cells == 0 || !gap.is_finite() || gap < NARROWEST_CELL {
        return None;
    }
    Some(gap / f64::from(cells))
}

/// The snap offset of a map, on one axis, from where its grid starts.
///
/// `at` is a line of the grid, in pixels from the edge of the image, and
/// `cell` the pixels of one cell. `scale` is the size of the map and
/// `step` the width of one canvas cell, in inches. The map comes out
/// that far past a point of the canvas grid, so a snapped move lays its
/// lines on the lines of the canvas. See `transform::snap_corner`.
pub fn offset(at: f64, cell: f64, scale: f64, step: f64) -> f64 {
    if cell <= 0.0 || step <= 0.0 || !at.is_finite() {
        return 0.0;
    }
    let start = at.rem_euclid(cell) / cell * scale;
    let offset = (-start).rem_euclid(step);
    if offset > step / 2.0 {
        offset - step
    } else {
        offset
    }
}

#[cfg(test)]
mod tests {
    use super::{Line, Way, cell_size, find, offset};

    const WHITE: u8 = 230;
    const INK: u8 = 60;

    /// A pale image, with `mark` deciding the gray of each pixel.
    fn image(width: u32, height: u32, mark: impl Fn(u32, u32) -> u8) -> Vec<u8> {
        (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .map(|(x, y)| mark(x, y))
            .collect()
    }

    fn of(lines: &[Line], way: Way) -> Vec<&Line> {
        lines.iter().filter(|line| line.way == way).collect()
    }

    #[test]
    fn a_drawn_grid_gives_its_lines_where_they_stand() {
        // Lines two pixels thick every 40 pixels, from column 10. The
        // middle of a pair of columns stands between them.
        let (width, height) = (400, 300);
        let pixels = image(width, height, |x, y| {
            if (x + 30) % 40 < 2 || (y + 30) % 40 < 2 {
                INK
            } else {
                WHITE
            }
        });
        let lines = find(&pixels, (width, height));
        let down = of(&lines, Way::Down);
        assert!(down.len() >= 9, "{down:?}");
        for line in &down {
            let off_grid = (line.at - 11.0).rem_euclid(40.0);
            assert!(off_grid.min(40.0 - off_grid) < 0.05, "{line:?}");
            assert!(line.strength > 0.9, "a line across the map: {line:?}");
        }
        let across = of(&lines, Way::Across);
        assert!(across.len() >= 6, "{across:?}");
    }

    #[test]
    fn a_finer_pattern_inside_each_tile_stays_a_weaker_line() {
        // A cell of 60 pixels, with a joint across its middle that runs
        // along half of each tile. The joint is a line too, and the DM
        // must see that the grid line is the stronger one.
        let (width, height) = (360, 240);
        let pixels = image(width, height, |x, y| {
            if x % 60 == 5 || (x % 60 == 35 && y % 60 < 30) {
                INK
            } else {
                WHITE
            }
        });
        let lines = find(&pixels, (width, height));
        let down = of(&lines, Way::Down);
        let grid: Vec<_> = down
            .iter()
            .filter(|line| line.at.rem_euclid(60.0) < 7.0)
            .collect();
        let joints: Vec<_> = down
            .iter()
            .filter(|line| (line.at.rem_euclid(60.0) - 35.5).abs() < 1.0)
            .collect();
        assert!(!grid.is_empty() && !joints.is_empty(), "{down:?}");
        let weakest_grid = grid.iter().map(|line| line.strength).fold(1.0, f32::min);
        let strongest_joint = joints.iter().map(|line| line.strength).fold(0.0, f32::max);
        assert!(weakest_grid > strongest_joint, "{grid:?} {joints:?}");
    }

    #[test]
    fn a_grid_inside_a_margin_still_stands_where_it_is() {
        // A plain frame of 40 pixels round a grid of 50.
        let (width, height) = (420, 320);
        let pixels = image(width, height, |x, y| {
            let inside = (40..width - 40).contains(&x) && (40..height - 40).contains(&y);
            if inside && x % 50 == 7 { INK } else { WHITE }
        });
        let lines = find(&pixels, (width, height));
        let down = of(&lines, Way::Down);
        assert!(down.len() >= 6, "{down:?}");
        for line in &down {
            assert!((line.at.rem_euclid(50.0) - 7.5).abs() < 0.05, "{line:?}");
        }
    }

    #[test]
    fn an_image_with_no_grid_gives_no_line() {
        // A soft light across the image, with a grain on top of it. A
        // cheap hash stands in for noise, so the test runs the same way
        // each time.
        let (width, height) = (300, 200);
        let pixels = image(width, height, |x, y| {
            let light = 90.0 + 80.0 * f64::from(x) / f64::from(width);
            let grain = f64::from((x.wrapping_mul(2_654_435_761) ^ y.wrapping_mul(40_503)) % 21);
            (light + grain - 10.0) as u8
        });
        assert_eq!(find(&pixels, (width, height)), Vec::new());
    }

    #[test]
    fn an_empty_image_gives_no_line() {
        assert!(find(&[], (0, 0)).is_empty());
        assert!(find(&[1, 2, 3], (10, 10)).is_empty());
    }

    #[test]
    fn two_lines_ten_cells_apart_give_one_cell() {
        let size = cell_size(11.0, 411.0, 10).unwrap();
        assert!((size - 40.0).abs() < 1e-9);
        // The order of the two picks says nothing.
        assert_eq!(cell_size(411.0, 11.0, 10), Some(size));
        assert_eq!(cell_size(11.0, 411.0, 0), None);
        assert_eq!(cell_size(11.0, 11.5, 1), None);
    }

    #[test]
    fn the_offset_lays_the_lines_of_the_map_on_the_canvas() {
        // A line a quarter of a cell in, on a map at true size, puts the
        // map a quarter of an inch before a point of the canvas grid.
        assert!((offset(10.0, 40.0, 1.0, 1.0) + 0.25).abs() < 1e-9);
        // Any line of the grid gives the same offset.
        assert!((offset(90.0, 40.0, 1.0, 1.0) + 0.25).abs() < 1e-9);
        // Past half a cell the offset goes the other way, to stay small.
        assert!((offset(30.0, 40.0, 1.0, 1.0) - 0.25).abs() < 1e-9);
        // A line on the edge needs no offset.
        assert!(offset(0.0, 40.0, 1.0, 1.0).abs() < 1e-9);
        // A map at twice the size moves its lines twice as far.
        assert!((offset(10.0, 40.0, 2.0, 1.0) - 0.5).abs() < 1e-9);
    }
}
