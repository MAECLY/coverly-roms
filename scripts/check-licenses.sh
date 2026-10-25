#!/usr/bin/env bash
#
# Fail if a crate in Cargo.lock has a licence that cannot be shipped inside
# Coverly ROMs (AGPL-3.0-only), or if the RAR crates appear at all.
#
# Rules, per package of `cargo metadata --locked` (every target, dev included):
# - The SPDX expression (legacy "/" read as OR) is evaluated: both sides of
#   an AND must be allowed, one side of an OR is enough; parentheses nest.
# - AGPL-3.0-only is allowed only for this workspace's own crates.
# - `unrar` and `unrar_sys` are forbidden whatever their licence: the UnRAR
#   licence forbids using the code to build a RAR compressor and is not
#   AGPL-compatible (spec 2026-10-23-organize-archives-and-discs.md §5.2).
#   RAR goes through an optional external tool instead.
# - A crate without a licence field fails.
set -euo pipefail

cd "$(dirname "$0")/.."

cargo metadata --format-version 1 --locked | python3 -I -c '
import json, re, sys

ALLOWED = {
    # Permissive and public-domain equivalents (spec §5.1).
    "MIT", "MIT-0", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC",
    "Zlib", "CC0-1.0", "Unicode-3.0", "BSL-1.0",
    "Apache-2.0 WITH LLVM-exception",
    # libbz2-rs-sys (pure-Rust bzip2 backend): BSD-style bzip2 licence.
    "bzip2-1.0.6",
    # webpki-root-certs (Mozilla CA list data), via the updater TLS stack.
    "CDLA-Permissive-2.0",
    # File-level copyleft, GPL-compatible (cssparser, selectors via Tauri).
    "MPL-2.0",
}
OWN_ONLY = {"AGPL-3.0-only"}
FORBIDDEN_CRATES = {"unrar", "unrar_sys", "unrar-sys"}

def allowed_expr(expr, allowed):
    """Evaluate an SPDX expression: OR needs one side, AND needs both."""
    tokens = re.findall(r"\(|\)|[^\s()]+", expr.replace("/", " OR "))
    pos = 0

    def peek():
        return tokens[pos] if pos < len(tokens) else None

    def take():
        nonlocal pos
        pos += 1
        return tokens[pos - 1]

    def atom():
        tok = take() if peek() is not None else None
        if tok == "(":
            value = disjunction()
            if take() != ")":
                raise ValueError("unbalanced parentheses")
            return value
        if tok in (None, ")", "AND", "OR", "WITH"):
            raise ValueError("unexpected token")
        if peek() == "WITH":
            take()
            tok = tok + " WITH " + take()
        return tok in allowed

    def conjunction():
        value = atom()
        while peek() == "AND":
            take()
            value = atom() and value
        return value

    def disjunction():
        value = conjunction()
        while peek() == "OR":
            take()
            value = conjunction() or value
        return value

    try:
        value = disjunction()
        return value and pos == len(tokens)
    except (ValueError, IndexError):
        return False

def ok(expr, own):
    return allowed_expr(expr, ALLOWED | (OWN_ONLY if own else set()))

meta = json.load(sys.stdin)
bad = []
for p in meta["packages"]:
    name, version = p["name"], p["version"]
    own = p.get("source") is None
    if name in FORBIDDEN_CRATES:
        bad.append(f"{name} {version}: forbidden crate (UnRAR licence, spec §5.2)")
        continue
    lic = p.get("license")
    if not lic:
        bad.append(f"{name} {version}: no licence field")
        continue
    if not ok(lic, own):
        bad.append(f"{name} {version}: licence not allowed: {lic}")

if bad:
    print("check-licenses: failed", file=sys.stderr)
    for line in sorted(bad):
        print("  " + line, file=sys.stderr)
    sys.exit(1)
count = len(meta["packages"])
print(f"check-licenses: ok ({count} packages)")
'
