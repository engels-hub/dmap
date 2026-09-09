#!/usr/bin/env python3
"""Turns the Lucide SVG files in crates/app/assets/icons into Rust polylines.

Run it from the repository root after a new glyph joins the assets folder:

    python3 tools/icons.py

It writes crates/app/src/icons.rs. Every point stays in the 24 x 24 grid of
the Lucide set, so the drawing code only scales and offsets them.
"""

import math
import os
import re
import sys

ASSETS = "crates/app/assets/icons"
OUT = "crates/app/src/icons.rs"
# Flatness of a curve, in the 24 unit grid. One tenth of a unit is under a
# tenth of a pixel at the 22 px the toolbar draws.
TOLERANCE = 0.1

NUM = re.compile(r"[-+]?(?:\d*\.\d+|\d+\.?)(?:[eE][-+]?\d+)?")
CMD = re.compile(r"([MmLlHhVvCcSsQqTtAaZz])")


def numbers(text):
    return [float(n) for n in NUM.findall(text)]


def cubic(p0, p1, p2, p3):
    """Flattens one cubic bezier into points, without its first point."""
    # The control net is never longer than the curve, so it bounds the error.
    net = dist(p0, p1) + dist(p1, p2) + dist(p2, p3)
    steps = max(2, min(48, int(math.ceil(net / TOLERANCE))))
    out = []
    for i in range(1, steps + 1):
        t = i / steps
        u = 1.0 - t
        x = u * u * u * p0[0] + 3 * u * u * t * p1[0] + 3 * u * t * t * p2[0] + t * t * t * p3[0]
        y = u * u * u * p0[1] + 3 * u * u * t * p1[1] + 3 * u * t * t * p2[1] + t * t * t * p3[1]
        out.append((x, y))
    return out


def dist(a, b):
    return math.hypot(b[0] - a[0], b[1] - a[1])


def arc(p0, rx, ry, rotation, large, sweep, p1):
    """Flattens one SVG elliptical arc into points, without its first point.

    The maths is the endpoint-to-center conversion of the SVG 1.1 spec,
    appendix F.6.
    """
    if rx == 0 or ry == 0 or p0 == p1:
        return [p1]
    phi = math.radians(rotation)
    cos, sin = math.cos(phi), math.sin(phi)
    dx, dy = (p0[0] - p1[0]) / 2.0, (p0[1] - p1[1]) / 2.0
    x1, y1 = cos * dx + sin * dy, -sin * dx + cos * dy
    rx, ry = abs(rx), abs(ry)
    # A radius too small for the two ends grows until it fits.
    lam = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry)
    if lam > 1:
        rx *= math.sqrt(lam)
        ry *= math.sqrt(lam)
    top = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1
    bottom = rx * rx * y1 * y1 + ry * ry * x1 * x1
    factor = math.sqrt(max(0.0, top / bottom)) * (-1 if large == sweep else 1)
    cx1, cy1 = factor * rx * y1 / ry, -factor * ry * x1 / rx
    cx = cos * cx1 - sin * cy1 + (p0[0] + p1[0]) / 2.0
    cy = sin * cx1 + cos * cy1 + (p0[1] + p1[1]) / 2.0
    start = math.atan2((y1 - cy1) / ry, (x1 - cx1) / rx)
    end = math.atan2((-y1 - cy1) / ry, (-x1 - cx1) / rx)
    sweep_angle = end - start
    if not sweep and sweep_angle > 0:
        sweep_angle -= 2 * math.pi
    elif sweep and sweep_angle < 0:
        sweep_angle += 2 * math.pi
    steps = max(2, min(64, int(math.ceil(abs(sweep_angle) * max(rx, ry) / TOLERANCE))))
    out = []
    for i in range(1, steps + 1):
        angle = start + sweep_angle * i / steps
        ex, ey = rx * math.cos(angle), ry * math.sin(angle)
        out.append((cos * ex - sin * ey + cx, sin * ex + cos * ey + cy))
    return out


def parse_path(d):
    """Reads one `d` attribute and returns its subpaths.

    Each subpath is a list of points and a flag that says whether `z` closed
    it.
    """
    parts = [p for p in CMD.split(d) if p.strip()]
    subpaths, points, closed = [], [], False
    at = (0.0, 0.0)
    start = (0.0, 0.0)
    last_control = None
    previous = ""
    i = 0
    while i < len(parts):
        command = parts[i]
        args = numbers(parts[i + 1]) if i + 1 < len(parts) and not CMD.fullmatch(parts[i + 1]) else []
        i += 2 if args else 1
        relative = command.islower()
        upper = command.upper()
        # A repeated argument set repeats the command. `M` repeats as `L`.
        step = {"M": 2, "L": 2, "H": 1, "V": 1, "C": 6, "S": 4, "Q": 4, "T": 2, "A": 7, "Z": 0}[upper]
        groups = [args[j : j + step] for j in range(0, len(args), step)] if step else [[]]
        for n, group in enumerate(groups):
            if upper == "Z":
                if points:
                    subpaths.append((points, True))
                points, closed = [], False
                at = start
                continue
            here = upper
            if upper == "M" and n > 0:
                here = "L"
            if here == "M":
                if points:
                    subpaths.append((points, False))
                x, y = group
                at = (at[0] + x, at[1] + y) if relative else (x, y)
                start = at
                points = [at]
            elif here in ("L", "T"):
                x, y = group
                to = (at[0] + x, at[1] + y) if relative else (x, y)
                if here == "T":
                    # A smooth quadratic mirrors the last control point.
                    control = at if previous.upper() not in ("Q", "T") or last_control is None \
                        else (2 * at[0] - last_control[0], 2 * at[1] - last_control[1])
                    points += quadratic(at, control, to)
                    last_control = control
                else:
                    points.append(to)
                at = to
            elif here == "H":
                at = (at[0] + group[0], at[1]) if relative else (group[0], at[1])
                points.append(at)
            elif here == "V":
                at = (at[0], at[1] + group[0]) if relative else (at[0], group[0])
                points.append(at)
            elif here in ("C", "S"):
                if here == "C":
                    c1 = offset(at, group[0], group[1], relative)
                    c2 = offset(at, group[2], group[3], relative)
                    to = offset(at, group[4], group[5], relative)
                else:
                    c1 = at if previous.upper() not in ("C", "S") or last_control is None \
                        else (2 * at[0] - last_control[0], 2 * at[1] - last_control[1])
                    c2 = offset(at, group[0], group[1], relative)
                    to = offset(at, group[2], group[3], relative)
                points += cubic(at, c1, c2, to)
                last_control, at = c2, to
            elif here == "Q":
                control = offset(at, group[0], group[1], relative)
                to = offset(at, group[2], group[3], relative)
                points += quadratic(at, control, to)
                last_control, at = control, to
            elif here == "A":
                to = offset(at, group[5], group[6], relative)
                points += arc(at, group[0], group[1], group[2], int(group[3]), int(group[4]), to)
                at = to
            previous = here
        if upper not in ("C", "S", "Q", "T"):
            last_control = None
    if points:
        subpaths.append((points, closed))
    return subpaths


def offset(at, x, y, relative):
    return (at[0] + x, at[1] + y) if relative else (x, y)


def quadratic(p0, control, p1):
    c1 = (p0[0] + 2.0 / 3.0 * (control[0] - p0[0]), p0[1] + 2.0 / 3.0 * (control[1] - p0[1]))
    c2 = (p1[0] + 2.0 / 3.0 * (control[0] - p1[0]), p1[1] + 2.0 / 3.0 * (control[1] - p1[1]))
    return cubic(p0, c1, c2, p1)


def rounded_rect(x, y, w, h, rx, ry):
    rx = min(rx, w / 2.0)
    ry = min(ry, h / 2.0)
    if rx <= 0 or ry <= 0:
        return [[(x, y), (x + w, y), (x + w, y + h), (x, y + h)]]
    d = (
        f"M{x + rx} {y} H{x + w - rx} A{rx} {ry} 0 0 1 {x + w} {y + ry} "
        f"V{y + h - ry} A{rx} {ry} 0 0 1 {x + w - rx} {y + h} "
        f"H{x + rx} A{rx} {ry} 0 0 1 {x} {y + h - ry} "
        f"V{y + ry} A{rx} {ry} 0 0 1 {x + rx} {y} Z"
    )
    return [points for points, _ in parse_path(d)]


def circle(cx, cy, r):
    steps = max(12, int(math.ceil(2 * math.pi * r / TOLERANCE)))
    return [[(cx + r * math.cos(2 * math.pi * i / steps), cy + r * math.sin(2 * math.pi * i / steps))
             for i in range(steps)]]


def attr(tag, name, default=0.0):
    found = re.search(rf'\b{name}="([^"]*)"', tag)
    return float(found.group(1)) if found else default


def read(path):
    """Returns every subpath of one SVG file as (points, closed)."""
    svg = open(path, encoding="utf-8").read()
    out = []
    for tag in re.findall(r"<(?:path|rect|circle|line|polyline|polygon)\b[^>]*>", svg):
        if tag.startswith("<path"):
            d = re.search(r'\bd="([^"]*)"', tag)
            out += parse_path(d.group(1))
        elif tag.startswith("<rect"):
            rx = attr(tag, "rx")
            ry = attr(tag, "ry", rx)
            for points in rounded_rect(attr(tag, "x"), attr(tag, "y"), attr(tag, "width"),
                                       attr(tag, "height"), rx, ry or rx):
                out.append((points, True))
        elif tag.startswith("<circle"):
            for points in circle(attr(tag, "cx"), attr(tag, "cy"), attr(tag, "r")):
                out.append((points, True))
        elif tag.startswith("<line"):
            out.append(([(attr(tag, "x1"), attr(tag, "y1")),
                         (attr(tag, "x2"), attr(tag, "y2"))], False))
        else:
            raw = numbers(re.search(r'\bpoints="([^"]*)"', tag).group(1))
            pairs = list(zip(raw[0::2], raw[1::2]))
            out.append((pairs, tag.startswith("<polygon")))
    return out


def thin(points):
    """Drops a point that sits on the line between its neighbours."""
    out = [points[0]]
    for point in points[1:]:
        if dist(out[-1], point) > 1e-4:
            out.append(point)
    if len(out) < 3:
        return out
    kept = [out[0]]
    for a, b in zip(out[1:-1], out[2:]):
        p = kept[-1]
        cross = (a[0] - p[0]) * (b[1] - p[1]) - (a[1] - p[1]) * (b[0] - p[0])
        if abs(cross) > 1e-3:
            kept.append(a)
    kept.append(out[-1])
    return kept


def name_of(file):
    return file[:-4].replace("-", "_")


def main():
    files = sorted(f for f in os.listdir(ASSETS) if f.endswith(".svg"))
    if not files:
        sys.exit(f"no SVG file in {ASSETS}")
    lines = [
        "//! The Lucide glyphs of DESIGN.md 4, flattened to polylines.",
        "//!",
        "//! `tools/icons.py` writes this file from `crates/app/assets/icons`.",
        "//! Do not edit it by hand. Every point sits in the 24 x 24 grid of the",
        "//! Lucide set. The set is under the ISC license; see the LICENSE file",
        "//! beside the SVG files.",
        "",
        "// Rust guideline compliant 2026-02-21",
        "",
        "/// One closed or open run of points in the 24 x 24 grid.",
        "pub struct Stroke {",
        "    /// The points of the run, in order.",
        "    pub points: &'static [(f32, f32)],",
        "    /// Whether the last point joins the first one.",
        "    pub closed: bool,",
        "}",
        "",
        "impl std::fmt::Debug for Stroke {",
        "    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {",
        '        f.debug_struct("Stroke").finish_non_exhaustive()',
        "    }",
        "}",
        "",
        "/// Every glyph the UI draws.",
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]",
        "pub enum Icon {",
    ]
    variants = []
    for file in files:
        variant = "".join(part.capitalize() for part in file[:-4].split("-"))
        variants.append((variant, file))
        lines.append(f"    /// The Lucide `{file[:-4]}` glyph.")
        lines.append(f"    {variant},")
    lines += ["}", "", "impl Icon {", "    /// The runs that draw the glyph.",
              "    pub fn strokes(self) -> &'static [Stroke] {", "        match self {"]
    for variant, file in variants:
        lines.append(f"            Self::{variant} => &{name_of(file).upper()},")
    lines += ["        }", "    }", "}", ""]
    for variant, file in variants:
        runs = [(thin(points), closed) for points, closed in read(os.path.join(ASSETS, file))]
        runs = [(points, closed) for points, closed in runs if len(points) > 1]
        lines.append(f"/// The `{file[:-4]}` glyph.")
        lines.append(f"static {name_of(file).upper()}: [Stroke; {len(runs)}] = [")
        for points, closed in runs:
            body = ", ".join(f"({x:.3f}, {y:.3f})" for x, y in points)
            lines.append(f"    Stroke {{ points: &[{body}], closed: {str(closed).lower()} }},")
        lines.append("];")
        lines.append("")
    open(OUT, "w", encoding="utf-8").write("\n".join(lines))
    print(f"{OUT}: {len(variants)} glyphs")


if __name__ == "__main__":
    main()
