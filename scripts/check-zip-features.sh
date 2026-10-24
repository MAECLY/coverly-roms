#!/usr/bin/env bash
#
# Fail if any crate in the dependency graph enables a `zip` feature that would
# let the `zip` crate decode LZMA, XZ, PPMd or zstd entries itself.
#
# Coverly reads those zip methods (14, 95, 98, 93) raw and decodes them with
# its own capped decoders in crates/coverly-core/src/archive (spec
# 2026-10-23-organize-archives-and-discs.md, S2 and S3). If the `zip` crate
# could decode them, a caller could bypass the decoder memory cap. The test
# `zip_crate_cannot_decode_lzma` (crates/coverly-core/tests/archive_zip.rs)
# checks the same thing at run time.
#
# Every version of `zip` in Cargo.lock is checked, for every target.
set -euo pipefail

cd "$(dirname "$0")/.."

forbidden='zip feature "(lzma|xz|ppmd|zstd|lzma-rs|deflate-zopfli|_deflate-any.*zopfli)"'

versions=$(cargo metadata --format-version 1 --locked 2>/dev/null |
  python3 -I -c 'import json, sys
for p in json.load(sys.stdin)["packages"]:
    if p["name"] == "zip" and p.get("source"):
        print(p["version"])' | sort -u)

if [ -z "$versions" ]; then
  echo "check-zip-features: no zip crate in Cargo.lock (nothing to check)"
  exit 0
fi

status=0
for v in $versions; do
  if hits=$(cargo tree --locked -e features -i "zip@$v" --target all 2>/dev/null |
    grep -E "$forbidden"); then
    echo "check-zip-features: zip $v has a forbidden feature enabled:" >&2
    echo "$hits" | sed 's/^/  /' | sort -u >&2
    echo "  Find who enables it: cargo tree -e features -i zip@$v --target all" >&2
    status=1
  fi
done

if [ "$status" -eq 0 ]; then
  echo "check-zip-features: ok (zip $(echo $versions): no lzma/xz/ppmd/zstd)"
fi
exit "$status"
