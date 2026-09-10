#!/usr/bin/env python3
"""Builds the font files that crates/app/assets/fonts holds.

The window draws in Atkinson Hyperlegible, which holds the Latin alphabet
and no more. Fira Sans stands behind it for the alphabets it lacks, and
this script cuts Fira down to those alphabets: the Latin letters would
never be reached, so carrying them twice only makes the binary larger.

DESIGN.md 3 asks for tabular figures. Both fonts put them behind the
OpenType `tnum` feature, and egui reads no OpenType feature. So this
script points the digits of the character map at their tabular shapes.

It needs the network and fontTools:

    python3 -m venv .venv && .venv/bin/pip install fonttools
    .venv/bin/python tools/fonts.py
"""

import io
import sys
import urllib.request

from fontTools import subset
from fontTools.ttLib import TTFont

OUT = "crates/app/assets/fonts"
WEIGHTS = ("Regular", "Bold")
DIGITS = "0123456789"

ATKINSON = "https://raw.githubusercontent.com/googlefonts/atkinson-hyperlegible/main"
FIRA = "https://raw.githubusercontent.com/google/fonts/main/ofl/firasans"

# What Fira Sans keeps: the alphabets Atkinson Hyperlegible does not hold,
# and the marks that go with them. Greek and Coptic, Cyrillic, Cyrillic
# Supplement, then the quotation marks and the dashes a translation uses.
KEEP = (
    list(range(0x0370, 0x0400))
    + list(range(0x0400, 0x0530))
    + [0x00AB, 0x00BB, 0x2010, 0x2013, 0x2014, 0x2018, 0x2019, 0x201C, 0x201D, 0x2026, 0x2116]
)

FAMILIES = (
    {
        "name": "Atkinson Hyperlegible",
        "font": ATKINSON + "/fonts/ttf/AtkinsonHyperlegible-{}.ttf",
        "file": OUT + "/AtkinsonHyperlegible-{}.ttf",
        "license": ATKINSON + "/OFL.txt",
        "license_file": OUT + "/LICENSE-OFL.txt",
        "keep": None,
    },
    {
        "name": "Fira Sans",
        "font": FIRA + "/FiraSans-{}.ttf",
        "file": OUT + "/FiraSans-{}.ttf",
        "license": FIRA + "/OFL.txt",
        "license_file": OUT + "/LICENSE-OFL-FiraSans.txt",
        "keep": KEEP,
    },
)


def tabular(font):
    """Returns the map from a proportional digit glyph to its tabular one."""
    out = {}
    for record in font["GSUB"].table.FeatureList.FeatureRecord:
        if record.FeatureTag != "tnum":
            continue
        for index in record.Feature.LookupListIndex:
            for table in font["GSUB"].table.LookupList.Lookup[index].SubTable:
                out.update(getattr(table, "mapping", {}))
    return out


def figures(font, name, weight):
    """Points the digits of the character map at their tabular shapes."""
    swap = tabular(font)
    if not swap:
        sys.exit(f"{name} {weight}: the font has no tnum feature")
    for table in font["cmap"].tables:
        for code, glyph in list(table.cmap.items()):
            if glyph in swap:
                table.cmap[code] = swap[glyph]


def cut(font, keep):
    """Drops every letter but the ones this font is carried for."""
    options = subset.Options()
    # A dropped table is a table egui never reads, and hinting is the
    # largest of them.
    options.drop_tables += ["FFTM"]
    options.hinting = False
    options.layout_features = []
    options.name_IDs = ["*"]
    options.notdef_outline = True
    cutter = subset.Subsetter(options=options)
    cutter.populate(unicodes=keep)
    cutter.subset(font)


def build(family):
    """Writes both weights of one family, and its license beside them."""
    for weight in WEIGHTS:
        raw = urllib.request.urlopen(family["font"].format(weight), timeout=60).read()
        font = TTFont(io.BytesIO(raw))
        figures(font, family["name"], weight)
        if family["keep"]:
            # The digits go with the letters: a number beside a Cyrillic
            # word must line up with the one beside a Latin one.
            cut(font, family["keep"] + [ord(digit) for digit in DIGITS] + [0x20])
        path = family["file"].format(weight)
        font.save(path)
        widths = {font["hmtx"][font.getBestCmap()[ord(digit)]][0] for digit in DIGITS}
        if len(widths) != 1:
            sys.exit(f"{path}: the digits still take {len(widths)} widths")
        letters = len(font.getBestCmap())
        print(f"{path}: {letters} letters, every digit {widths.pop()} units wide")
    text = urllib.request.urlopen(family["license"], timeout=60).read()
    open(family["license_file"], "wb").write(text)
    print(family["license_file"])


def main():
    for family in FAMILIES:
        build(family)


if __name__ == "__main__":
    main()
