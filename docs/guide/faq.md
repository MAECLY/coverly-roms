# Questions

[Coverly ROMs](../../README.md) › Questions · **English** · [Español](faq.es.md)

### Is it free?

Yes. It costs nothing, needs no account and has no paid tier. The code is
AGPL-3.0.

### Does it come with ROMs or cover art?

No. Coverly ROMs bundles no ROMs, BIOS files or game art. Images are downloaded
from libretro-thumbnails and, if you add your own key, SteamGridDB. Every image
belongs to its owners.

### Does it need console keys?

No. Every system is identified from what is readable without keys: plain
headers, file tables, the title ID and the file name. A Switch or 3DS game
whose name cannot be read that way is labelled "Encrypted". Files such as
`prod.keys` or `title.keys` are never opened or moved.

### Will it overwrite my covers?

Not unless you ask it to. By default it only fills what is missing. If you
choose to replace art, it first keeps a copy in the app's data folder, not on
the card, and Undo puts it back.

### Do I need an account?

No. libretro-thumbnails works without one. SteamGridDB is optional and uses
your own key, kept in your system keychain; add it in Settings → Integrations.

### Can it open 7z and RAR archives?

Yes. Zip, 7z, tar, gzip, bzip2, xz, zstd and lz4 open with nothing else
installed. RAR needs a program that is already on many computers, because the
official RAR code cannot ship in a free program: `unrar`, 7-Zip (`7zz` or `7z`)
or `bsdtar`, which macOS already includes (`/usr/bin/bsdtar`). On Windows,
7-Zip or WinRAR in their usual folders are found; on Linux, install `unrar`,
`7zip` or `libarchive-tools`. Without one, a RAR stays where it is with the note
"To open RAR archives, install unrar" and the rest is organised. Settings →
Organise → _Advanced options_ → _External programs for RAR_ shows what was
found and can turn them off.

Disc games are extracted, because handhelds load `.cue`/`.gdi` and tracks, not
archives. Cartridge ROMs in a zip or 7z are moved as they are when the layout
loads them; other containers are extracted. In move mode the archive goes to
the Trash only after every file has been verified, and only when nothing else
is left in it (a save file inside keeps the archive). Undo removes what was
extracted and brings the archive back from the Trash.

### Which handhelds does it work with?

Any that uses the Anbernic stock, GarlicOS, Onion OS, MinUI and NextUI, muOS,
ES-DE, Batocera, Knulli and EmulationStation, or RetroArch layout. For other
firmware you can build your own layout with a live preview. It has not been
tested on real handhelds yet.

### Why do Gatekeeper and SmartScreen warn me?

Because the downloads carry no Apple or Microsoft certificate yet. The first
time, on macOS clear the quarantine flag or click **Open Anyway**; on Windows,
choose **More info**, then **Run anyway**. Only do it for files from this
project's releases. The steps are under [Download](../../README.md#download).

### Does it send data anywhere?

There is no telemetry. The app only talks to the art sources you enable, to
the services you set up in Settings → Integrations, to the Switch name
catalogue if you turn it on and, unless you turn it off, to the update check at
startup. Each integration card says exactly what it sends. Images are
re-encoded, so they carry no metadata.

### How does it update?

From inside the app, with GitHub Releases builds verified with minisign before
they install. There are stable and beta channels, and it never restarts in the
middle of a job. The command line uses `coverly update`.

## What it doesn't do yet

- ScreenScraper as an art source: planned, not available yet.
- Videos, mixed images, marquees or wheels.
- Editing metadata, renaming or verifying ROMs.
- Running on the handheld itself.
- It has not been tested on real handhelds yet.
- The builds are not signed with an Apple or Microsoft certificate yet.

## Compared with other tools

Skraper, Skyscraper and ARRM use ScreenScraper and bring videos, mixed images
and metadata; Coverly ROMs only fills images. Mini Scraper also works without
an account and already has releases out. Where Coverly ROMs differs: a native
build for macOS (Apple silicon and Intel), Windows and Linux (x64 and ARM64),
the Anbernic `Imgs/` layout built in, no account, and a dry run and undo for
every job. The [comparison on the website](https://coverly-roms.maecly.com/en/#compare)
lists each tool's sources.
