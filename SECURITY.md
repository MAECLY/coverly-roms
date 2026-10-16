# Security Policy

## Supported versions

Coverly ROMs is in pre-alpha. Once the first stable release ships, this section
will list which versions receive security fixes.

| Version | Status                                 |
| ------- | -------------------------------------- |
| 0.x     | Pre-alpha. No security guarantees yet. |

## Reporting a vulnerability

**Do not open a public GitHub issue for security reports.**

Use one of these private channels:

1. **GitHub Security Advisories** — preferred. Open a private advisory at:
   `https://github.com/MAECLY/coverly-roms/security/advisories/new`
2. **Email** — `security@maecly.com`.

Please include:

- A description of the issue and its impact.
- Steps to reproduce, ideally with a minimal proof of concept.
- The affected version, OS, and any relevant logs (with paths redacted).
- Whether you would like credit in the eventual disclosure.

## Response timeline

- **48 hours**: initial acknowledgement.
- **5 business days**: triage assessment with severity (CVSS-aligned).
- **30 days**: target for a fix in mainline. Some issues take longer; we will keep you informed.
- **90 days**: maximum embargo before coordinated public disclosure, unless mutually extended.

## What is in scope

- The Tauri application (Rust core + React frontend).
- The `coverly` command-line tool and the `coverly-core` library.
- Anything that writes to a user's library: art files, `gamelist.xml` merges,
  backups and undo. A path that escapes the library root is a security bug.
- Decoding of downloaded images and parsing of remote indexes and manifests.
- The auto-updater pipeline (manifest and minisign signature verification).
- Storage of source credentials (API keys in the OS keychain).

## What is out of scope

- Vulnerabilities in third-party dependencies that are already publicly disclosed (please report upstream).
- Vulnerabilities in the art sources themselves (libretro-thumbnails, ScreenScraper, SteamGridDB, TheGamesDB); report those to their maintainers.
- Self-XSS or social-engineering attacks that require the user to paste arbitrary code into a console.
- Issues that only affect unsupported pre-alpha builds older than 90 days.

## Acknowledgements

Reporters who follow this policy and act in good faith will be credited in the release notes (unless they opt out).
