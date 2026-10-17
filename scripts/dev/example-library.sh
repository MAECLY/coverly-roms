#!/usr/bin/env bash
# Build a small throwaway ROM library in the Anbernic stock layout
# (`ROMS/<SYSTEM>/Imgs/<stem>.png`) to try the CLI and the app on.
#
#   scripts/dev/example-library.sh [DIR]   (default: target/example/library)
#
# The ROMs are a few bytes of filler with real No-Intro names, so matching
# falls back to the name. One ROM per system already has a cover, so an audit
# shows both states. Re-running starts from a clean copy.
set -euo pipefail

dir="${1:-target/example/library}"
case "$dir" in
  '' | / | "$HOME" | /Volumes/*)
    echo "example-library: refusing to write to '$dir'" >&2
    exit 2
    ;;
esac

rm -rf "$dir"

# A 1x1 PNG, enough for the audit to count a cover as present.
png='iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg=='

rom() { # rom <system folder> <file name>
  mkdir -p "$dir/ROMS/$1"
  printf 'coverly example rom: %s\n' "$2" >"$dir/ROMS/$1/$2"
}

cover() { # cover <system folder> <stem>
  mkdir -p "$dir/ROMS/$1/Imgs"
  printf '%s' "$png" | base64 --decode >"$dir/ROMS/$1/Imgs/$2.png"
}

rom GBA 'Golden Sun (USA, Europe).gba'
rom GBA 'Metroid Fusion (USA).gba'
rom GBA 'Advance Wars (USA).gba'
cover GBA 'Golden Sun (USA, Europe)'

rom SFC 'Super Metroid (Japan, USA) (En,Ja).sfc'
rom SFC 'Chrono Trigger (USA).sfc'
rom SFC 'Donkey Kong Country (USA) (Rev 2).sfc'
cover SFC 'Chrono Trigger (USA)'

rom GB 'Tetris (World) (Rev 1).gb'
rom GB 'Pokemon - Red Version (USA, Europe) (SGB Enhanced).gb'
cover GB 'Tetris (World) (Rev 1)'

rom MD 'Sonic The Hedgehog (USA, Europe).md'
rom MD 'Streets of Rage 2 (USA).md'

count=$(find "$dir/ROMS" -type f ! -path '*/Imgs/*' | wc -l | tr -d ' ')
echo "example library: $dir ($count ROMs in 4 systems, 3 with a cover)"
