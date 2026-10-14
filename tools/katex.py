"""Packs the KaTeX fonts formulas are drawn with:

- crates/app/math/katex.br: the fonts, Brotli-compressed: a first line of
  their names and sizes (`Main-Regular:53580 Math-Italic:31308 ...`),
  then the fonts themselves one after the other.
- crates/app/math/LICENSE.txt: KaTeX's licence.

They are the fonts RaTeX lays formulas out for: its metrics are theirs.

    pip install brotli
    python3 tools/katex.py
"""

import pathlib
import urllib.request

import brotli

KATEX = "https://cdn.jsdelivr.net/npm/katex@0.16.22"
OUT = pathlib.Path(__file__).resolve().parent.parent / "crates/app/math"
FONTS = [
    "Main-Regular", "Main-Bold", "Main-Italic", "Main-BoldItalic", "Math-Italic", "Math-BoldItalic",
    "AMS-Regular", "Size1-Regular", "Size2-Regular", "Size3-Regular", "Size4-Regular",
    "Caligraphic-Regular", "Fraktur-Regular", "Fraktur-Bold", "Script-Regular",
    "SansSerif-Regular", "SansSerif-Bold", "SansSerif-Italic", "Typewriter-Regular",
]


def get(url):
    with urllib.request.urlopen(url) as r:
        return r.read()


fonts = [get(f"{KATEX}/dist/fonts/KaTeX_{name}.ttf") for name in FONTS]
head = " ".join(f"{name}:{len(data)}" for name, data in zip(FONTS, fonts)) + "\n"
OUT.mkdir(exist_ok=True)
(OUT / "katex.br").write_bytes(brotli.compress(head.encode() + b"".join(fonts), quality=11))
(OUT / "LICENSE.txt").write_bytes(get(f"{KATEX}/LICENSE"))
