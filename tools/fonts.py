#!/usr/bin/env python3
"""Builds the Atkinson Hyperlegible files that crates/app/assets/fonts holds.

DESIGN.md 3 asks for tabular figures. The upstream font puts them behind the
OpenType `tnum` feature, and egui reads no OpenType feature. So this script
points the digits of the character map at their tabular shapes, and writes
the result beside the license.

It needs the network and fontTools:

    python3 -m venv .venv && .venv/bin/pip install fonttools
    .venv/bin/python tools/fonts.py
"""

import io
import sys
import urllib.request

from fontTools.ttLib import TTFont

URL = "https://raw.githubusercontent.com/googlefonts/atkinson-hyperlegible/main/fonts/ttf/AtkinsonHyperlegible-{}.ttf"
LICENSE = "https://raw.githubusercontent.com/googlefonts/atkinson-hyperlegible/main/OFL.txt"
OUT = "crates/app/assets/fonts"
WEIGHTS = ("Regular", "Bold")
DIGITS = "0123456789"


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


def main():
    for weight in WEIGHTS:
        raw = urllib.request.urlopen(URL.format(weight), timeout=60).read()
        font = TTFont(io.BytesIO(raw))
        swap = tabular(font)
        if not swap:
            sys.exit(f"{weight}: the font has no tnum feature")
        for table in font["cmap"].tables:
            for code, glyph in list(table.cmap.items()):
                if glyph in swap:
                    table.cmap[code] = swap[glyph]
        path = f"{OUT}/AtkinsonHyperlegible-{weight}.ttf"
        font.save(path)
        widths = {font["hmtx"][font.getBestCmap()[ord(d)]][0] for d in DIGITS}
        if len(widths) != 1:
            sys.exit(f"{weight}: the digits still take {len(widths)} widths")
        print(f"{path}: every digit is {widths.pop()} units wide")
    text = urllib.request.urlopen(LICENSE, timeout=60).read()
    open(f"{OUT}/LICENSE-OFL.txt", "wb").write(text)
    print(f"{OUT}/LICENSE-OFL.txt")


if __name__ == "__main__":
    main()
