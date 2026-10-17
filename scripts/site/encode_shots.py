#!/usr/bin/env python3
"""Encode the landing screenshots into site/assets/shots/.

  encode_shots.py RAW_DIR [--out site/assets/shots]

RAW_DIR holds the captures as <lang>/<screen>-<light|dark>.png, taken from
the browser UI with its mock backend (`make web`) at a 1600x1000 viewport
and device pixel ratio 2, so each PNG is 3200x2000. Each becomes
<lang>/<screen>-<mode>.webp (2560 wide) and <lang>/<screen>-<mode>@1x.webp
(1280 wide), the sizes the <picture> srcsets name.

The mock draws labelled placeholder art, so no capture shows real box art.
Needs `cwebp` (libwebp) on PATH.
"""

from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
from pathlib import Path

WIDTHS = {"": 2560, "@1x": 1280}
QUALITY = "82"


def main(argv: list[str]) -> int:
    p = argparse.ArgumentParser(description=(__doc__ or "").split("\n\n")[0])
    p.add_argument("raw", type=Path)
    p.add_argument("--out", type=Path, default=Path("site/assets/shots"))
    a = p.parse_args(argv)
    if shutil.which("cwebp") is None:
        print("encode_shots: cwebp not found (install libwebp)", file=sys.stderr)
        return 1
    pngs = sorted(a.raw.glob("*/*-light.png")) + sorted(a.raw.glob("*/*-dark.png"))
    if not pngs:
        print(f"encode_shots: no <lang>/<screen>-<mode>.png in {a.raw}", file=sys.stderr)
        return 1
    for png in pngs:
        lang = png.parent.name
        (a.out / lang).mkdir(parents=True, exist_ok=True)
        for suffix, width in WIDTHS.items():
            dest = a.out / lang / f"{png.stem}{suffix}.webp"
            subprocess.run(
                ["cwebp", "-quiet", "-q", QUALITY, "-m", "6", "-sharp_yuv",
                 "-metadata", "none", "-resize", str(width), "0",
                 str(png), "-o", str(dest)],
                check=True,
            )
            print(f"{dest} ({dest.stat().st_size // 1024} KiB)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
