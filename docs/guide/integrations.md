# Integrations, privacy and safety

[Coverly ROMs](../../README.md) › Integrations, privacy and safety · **English** · [Español](integrations.es.md)

## Integrations

Coverly works without any account. Settings → Integrations is where you add
the optional ones, and turn the online services on or off:

| Service               | What it adds                                              | What it needs                                                                                           |
| --------------------- | --------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| SteamGridDB           | Community covers in Fill, and the votes behind Popularity | Your own API key, from [SteamGridDB's API page](https://www.steamgriddb.com/profile/preferences/api)    |
| ScreenScraper         | Identifying files by hash when organising                 | A free account, from [ScreenScraper's sign-up page](https://www.screenscraper.fr/membreinscription.php) |
| Switch name catalogue | Switch game names, publisher and date from the title ID   | Nothing; it stays off until you turn it on                                                              |
| Coverly updates       | Checks GitHub for a new version of Coverly ROMs           | Nothing; turn it off in Settings → General                                                              |

ScreenScraper also needs developer credentials built into the app, and the
published versions do not carry them yet: its card shows "Not available", and
you can save your account for when it arrives.

**Getting a key.** "How to get the key" on each card opens the service's own
account page, never a third-party site. SteamGridDB: sign in with Steam, open
Preferences › API, generate a key and paste it into the card. ScreenScraper:
create a free account and enter its user name and password. Then press
**Test connection**: the card says whether it works, whether the key is
wrong, whether you are offline or the service is down and, for
ScreenScraper (in builds that include it), how much of the daily quota is
used.

**What is sent.** Each card says it. SteamGridDB gets the name of the game
being looked up and your key; ScreenScraper gets the file name, its size, its
hashes and the system, with your account, over HTTPS (never the full path or
the file contents); the Switch catalogue and the update check download public
files and send nothing from your library, except the eShop icons (requested one
game at a time) and the online fallback (the missing title ID) when you turn
them on.

**Where keys are kept.** In the OS keychain: Keychain on macOS, Credential
Manager on Windows and the Secret Service (GNOME Keyring or KWallet) on Linux.
A saved key is never shown again, not in the app, the logs, the settings
export or the CLI; they only say whether one is set. "Forget credentials"
deletes it from the keychain, and turning a service off keeps it for later.

**From the command line.** `coverly integrations` uses the same list and the
same keychain entries as the app:

```bash
coverly integrations list                          # every service, on or off, and which keys are set
coverly integrations set steamgriddb               # asks for the key, without showing what you type
coverly integrations set steamgriddb --from-env COVERLY_SGDB_KEY  # or reads it from a variable
coverly integrations set screenscraper --field user
coverly integrations set screenscraper --field password
coverly integrations test steamgriddb              # one request with the saved key
coverly integrations forget steamgriddb            # asks first; --yes without a terminal
```

`set` never takes the secret as an argument, so it does not end up in your
shell history; type it at the hidden prompt, pipe it in, or name a variable
with `--from-env`. The flags and exit codes are under
[Command line](command-line.md).

## Privacy and safety

- **No telemetry, no account.** The app only talks to the art sources you
  enable, to the services you set up in Settings → Integrations, to the Switch
  name catalogue if you turn it on and, unless you turn it off, to the update
  check. Each integration card says what it sends.
- **No console keys.** Coverly never reads, asks for, downloads or ships
  them; key and licence files on the card are left untouched.
- **Images are re-encoded** before they are written, so they carry no metadata.
- **Your keys stay in the OS keychain.** The SteamGridDB key is sent only to
  SteamGridDB, as an `Authorization: Bearer` header, and the ScreenScraper
  account only to ScreenScraper. Nothing shows a saved key again, and logs
  only say whether one is set. Settings, their export and their import never
  carry a key or a password, and `coverly integrations` never prints one.
  Opening Settings sends nothing; "Test connection" makes one request when you
  press it.
- **The audit only reads.** Fill, organise and duplicates write nothing until
  you apply, never outside the library root, and every job can be undone.
  Duplicates are moved aside or to the Trash, never deleted.
- **Backups and job logs** go to the app's data folder, not to the SD card.
- **No ROMs, BIOS files or game art** ship with Coverly ROMs.
