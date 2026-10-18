#!/usr/bin/env bash
# Builds the landing for Vercel (vercel.json → buildCommand), the same way
# the old GitHub Pages job did: check site/, then fill its {{release.*}}
# markers from the latest published release into target/site.
#
# The latest release comes from the public GitHub API. GITHUB_TOKEN, when the
# Vercel project defines it, only raises the rate limit. A 404 (no release
# yet) ships the not-published-yet state; any other API error fails the
# build, so a deploy never links a file that is not there.
set -euo pipefail

REPO="MAECLY/coverly-roms"
OUT="target/site"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

python3 -I scripts/site/check_site.py

auth=()
if [ -n "${GITHUB_TOKEN:-}" ]; then
  auth=(-H "Authorization: Bearer $GITHUB_TOKEN")
fi
status=$(curl -sS -o "$TMP/release.json" -w '%{http_code}' \
  -H "Accept: application/vnd.github+json" ${auth[@]+"${auth[@]}"} \
  "https://api.github.com/repos/$REPO/releases/latest")

rm -rf "$OUT"
case "$status" in
  200)
    python3 -I scripts/site/apply_release.py --release-json "$TMP/release.json" \
      --repo "$REPO" --site site --out "$OUT"
    ;;
  404)
    echo "No published release yet: building the not-published-yet state."
    python3 -I scripts/site/apply_release.py --no-release \
      --repo "$REPO" --site site --out "$OUT"
    ;;
  *)
    echo "GitHub API answered $status:" >&2
    cat "$TMP/release.json" >&2
    exit 1
    ;;
esac
# GitHub Pages files that mean nothing on Vercel.
rm -f "$OUT/CNAME" "$OUT/.nojekyll"
