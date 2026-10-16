# ADR-0010: Safe writes, with backups and journals in the app data dir

## Status

Accepted (implemented in M2)

## Date

2026-10-16

## Context

Coverly ROMs writes into users' SD cards: FAT32 or exFAT volumes that are case-insensitive, can be pulled out mid-write, and are shared with handhelds that list every file they find. Spec §10, §11 and §16.10 require these guarantees: never write outside the library, write atomically, use FAT-safe names, overwrite nothing by default, back up whatever is replaced, make every job undoable, and create no `._*` AppleDouble files. The obvious place for backups and journals, a folder on the card, would show up on the handheld and use the card's space.

## Decision

- **Atomic writes.** Every library write goes through `write::Writer`. The bytes go to a temp file next to the target, which is fsynced and then renamed over the target. A failed write leaves no temp file.
- **Containment.** Each target passes `ensure_within(root, target)`, lexically and through symlinks. Names must already be FAT-safe. File names always come from the ROM stem through the profile template, never from the network.
- **Overwrite policy.** The default is `never`, and a case-insensitive twin counts as existing. `backup` copies the old bytes before replacing them.
- **Backups and journals live in the app data dir**, not on the card: `<data>/backups/<job>/<relative path>` and `<data>/jobs/<job>.jsonl` (OS data dir, or `--data-dir`). The journal stores the absolute library root, then one entry per write (target, `created` or `replaced` with its backup, SHA1 of the written bytes), and is fsynced per entry.
- **Undo** reverses a job's entries in reverse order. It removes or restores a file only while its SHA1 still matches what the job wrote; files changed later are kept and reported. The newest 10 jobs are kept.
- **No xattrs.** The writer never sets extended attributes and never uses `fs::copy`, which copies xattrs on macOS. macOS still attaches `com.apple.provenance` to every file a process creates, and user space cannot remove it. On FAT/exFAT that attribute becomes a `._<name>` file, so the writer deletes the `._` sidecar of every file and folder it creates. It never deletes a sidecar that existed before the write.

## Consequences

### Positive

- The handheld never sees backup or journal folders, and the card's free space is not used for them.
- `coverly undo` works from any working directory and after a crash or a pause, because the journal is fsynced per entry and stores an absolute root.
- A user's later edits are never destroyed by undo.
- Verified on a real FAT32 image: 0 `._*` files created and 0 files left after undo (M2 end-to-end run).

### Negative

- Backups are tied to the computer that ran the job. Undo on another computer is not possible.
- Two extra fsyncs per file make writes slower on slow cards.

### Risks / Open Questions

- If the user deletes the app data dir, the jobs before that point cannot be undone. The CLI prints the job id and the M4 job screen will show where backups live.
- If macOS starts storing other non-removable attributes, the sidecar cleanup still covers them: it deletes the file, not individual attributes.
