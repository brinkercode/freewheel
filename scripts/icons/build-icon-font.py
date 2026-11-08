#!/usr/bin/env python3
"""Builds the profile icon font and its name list (a maintainer tool; CI does not run it).

Input: Google's static Material Symbols Rounded woff2 at weight 400, fill 0,
grade 0, optical size 24 (the style of the UI's SVG icons), downloaded once
by hand from Google Fonts. The app itself never fetches it.

    python3 -m venv /tmp/ft && /tmp/ft/bin/pip install fonttools brotli
    /tmp/ft/bin/python scripts/icons/build-icon-font.py <rounded.woff2> \
        src/assets/symbols/symbols.woff2 crates/freewheel-core/src/icon_names.txt

Every icon is kept. The ligature tables (GSUB) and the Latin letters they
need are dropped, because the app draws an icon by its codepoint; the
name -> codepoint list is read from those ligatures first and written next
to the Rust code that checks names (one "name codepoint" per line, sorted).
NOTICE credits the font (Apache-2.0); update its version there.
"""
import sys

from fontTools import subset
from fontTools.ttLib import TTFont


def private_use(cp):
    return 0xE000 <= cp <= 0xF8FF or cp >= 0xF0000


def main(src, out_font, out_names):
    font = TTFont(src)
    cmap = font.getBestCmap()
    codepoint = {}
    for cp, glyph in sorted(cmap.items()):
        if private_use(cp):
            codepoint.setdefault(glyph, cp)
    letter = {}
    for cp, glyph in sorted(cmap.items()):
        ch = chr(cp)
        if cp < 0x80 and (glyph not in letter or ch.islower()):
            letter[glyph] = ch
    names = {}
    for lookup in font["GSUB"].table.LookupList.Lookup:
        for table in lookup.SubTable:
            if lookup.LookupType == 7:
                table = table.ExtSubTable
            for first, ligatures in getattr(table, "ligatures", {}).items():
                for lig in ligatures:
                    name = letter.get(first, "?") + "".join(letter.get(g, "?") for g in lig.Component)
                    if name == name.lower() and "?" not in name and lig.LigGlyph in codepoint:
                        names[name] = codepoint[lig.LigGlyph]
    print(f"{len(names)} names, {len(set(names.values()))} glyphs")
    options = subset.Options()
    options.layout_features = []
    options.hinting = False
    options.name_IDs = ["*"]
    options.name_languages = ["*"]
    options.notdef_outline = True
    options.flavor = "woff2"
    options.drop_tables += ["GSUB", "GPOS", "GDEF", "STAT", "fvar", "gvar", "avar", "HVAR", "MVAR"]
    subsetter = subset.Subsetter(options)
    subsetter.populate(unicodes=sorted(set(names.values())))
    subsetter.subset(font)
    font.flavor = "woff2"
    font.save(out_font)
    with open(out_names, "w", newline="\n") as f:
        for name in sorted(names):
            f.write(f"{name} {names[name]:x}\n")


if __name__ == "__main__":
    main(*sys.argv[1:4])
