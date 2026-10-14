"""Builds crates/app/emoji/twemoji.png: the page icons the app offers, drawn
by Twemoji, in one row-major grid of 128 px cells in the order of ICONS in
crates/app/src/emoji.rs.

    pip install resvg-py pillow
    python3 tools/twemoji.py
"""

import io
import pathlib
import re
import urllib.request

import resvg_py
from PIL import Image

VERSION = "15.1.0"
CELL, COLUMNS = 128, 8
ROOT = pathlib.Path(__file__).resolve().parent.parent

source = (ROOT / "crates/app/src/emoji.rs").read_text()
icons = re.search(r'^pub const ICONS: &str = "(.*)";$', source, re.M).group(1)
rows = -(-len(icons) // COLUMNS)
atlas = Image.new("RGBA", (COLUMNS * CELL, rows * CELL))
for i, c in enumerate(icons):
    name = "-".join(f"{ord(p):x}" for p in c if p != "️")
    url = f"https://cdn.jsdelivr.net/gh/jdecked/twemoji@{VERSION}/assets/svg/{name}.svg"
    svg = urllib.request.urlopen(url).read().decode()
    png = resvg_py.svg_to_bytes(svg_string=svg, width=CELL, height=CELL)
    cell = Image.open(io.BytesIO(bytes(png)))
    atlas.paste(cell, (i % COLUMNS * CELL, i // COLUMNS * CELL))
atlas.save(ROOT / "crates/app/emoji/twemoji.png", optimize=True)
