"""Builds the emoji the app draws, from Twemoji and Emojibase:

- crates/app/emoji/emoji.txt: the emoji the picker offers, in order, one
  per line: category, emoji, name, search words, and its five skin tones
  (light to dark) when it has them, separated by tabs.
- crates/app/emoji/twemoji.br: their Twemoji images, Brotli-compressed
  lines of an emoji, a tab and its SVG.

    pip install brotli
    python3 tools/twemoji.py
"""

import concurrent.futures
import json
import pathlib
import urllib.request

import brotli

TWEMOJI = "https://cdn.jsdelivr.net/gh/jdecked/twemoji@15.1.0"
EMOJIBASE = "https://cdn.jsdelivr.net/npm/emojibase-data@16.0.3"
OUT = pathlib.Path(__file__).resolve().parent.parent / "crates/app/emoji"
# Emojibase's groups as the picker's categories; components (skin tones,
# hair) are not offered on their own.
CATEGORIES = {0: "people", 1: "people", 3: "nature", 4: "food", 5: "travel", 6: "activity", 7: "objects", 8: "symbols", 9: "flags"}
# Emojibase numbers travel before activities; the picker puts activities
# first, as Notion does.
GROUP_ORDER = [0, 1, 3, 4, 6, 5, 7, 8, 9]
TONES = ["1f3fb", "1f3fc", "1f3fd", "1f3fe", "1f3ff"]


def get(url):
    with urllib.request.urlopen(url) as r:
        return r.read()


listing = json.loads(get("https://data.jsdelivr.com/v1/packages/gh/jdecked/twemoji@15.1.0?structure=flat"))
files = {f["name"].rsplit("/", 1)[1][:-4] for f in listing["files"] if f["name"].startswith("/assets/svg/")}


def twemoji_name(hexcode):
    """Twemoji's file for an emoji: its code points, without the emoji
    variation selector when Twemoji leaves it out."""
    name = hexcode.lower()
    if name in files:
        return name
    name = "-".join(p for p in name.split("-") if p != "fe0f")
    return name if name in files else None


entries = []
for e in json.loads(get(f"{EMOJIBASE}/en/compact.json")):
    if e.get("group") not in CATEGORIES or not twemoji_name(e["hexcode"]):
        continue
    # Only the skins of one tone throughout, one for each tone, in order.
    skins = {}
    for s in e.get("skins", []):
        tones = {p for p in s["hexcode"].lower().split("-") if p in TONES}
        if len(tones) == 1 and twemoji_name(s["hexcode"]):
            skins.setdefault(tones.pop(), s)
    skins = [skins[t] for t in TONES] if len(skins) == len(TONES) else []
    entries.append((GROUP_ORDER.index(e["group"]), e["order"], e, skins))
entries.sort(key=lambda x: x[:2])

lines, images = [], {}
for _, _, e, skins in entries:
    words = " ".join(dict.fromkeys(w for w in e.get("tags", []) if w not in e["label"].split()))
    fields = [CATEGORIES[e["group"]], e["unicode"], e["label"], words] + [s["unicode"] for s in skins]
    assert not any("\t" in f or "\n" in f for f in fields)
    lines.append("\t".join(fields))
    for x in [e] + skins:
        images[x["unicode"]] = twemoji_name(x["hexcode"])

with concurrent.futures.ThreadPoolExecutor(32) as pool:
    svgs = dict(zip(images, pool.map(lambda n: get(f"{TWEMOJI}/assets/svg/{n}.svg").decode(), images.values())))
blob = "".join(f"{u}\t{' '.join(svg.split())}\n" for u, svg in svgs.items())
(OUT / "emoji.txt").write_text("\n".join(lines) + "\n")
(OUT / "twemoji.br").write_bytes(brotli.compress(blob.encode(), quality=11, lgwin=24))
print(f"{len(lines)} emoji, {len(svgs)} images")
