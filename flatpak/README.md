# Flatpak (Flathub)

Snook Legend's ID on Flathub is `app.snooklegend.SnookLegend`. This folder
has the files the game itself provides for it (and for any other Linux
package):

| File | What it is |
|---|---|
| `app.snooklegend.SnookLegend.metainfo.xml` | The page in software centers and on Flathub: description, screenshots, age rating (OARS), releases. Install it to `share/metainfo/`. |
| `app.snooklegend.SnookLegend.desktop` | The launcher. Install it to `share/applications/`. |

The icons are the emblem's PNGs, `crates/client/assets/icon/snook-legend-<size>.png`
(64, 128, 256 and 512 px), installed as
`share/icons/hicolor/<size>x<size>/apps/app.snooklegend.SnookLegend.png`.

The MetaInfo's releases are written from "What's new" by
`python3 tools/gen_metainfo.py` (see [docs/RELEASING.md](../docs/RELEASING.md)).
The client's tests check that the ID, the executable (`snook-legend`) and the
window's class (`StartupWMClass`, set in `main.rs`) match, and that every
version of "What's new" is in the MetaInfo.

**Flathub's manifest and the submission are written by hand.** Flathub's
[Generative AI policy](https://docs.flathub.org/docs/for-app-authors/requirements#generative-ai-policy)
does not accept manifests with AI-generated or AI-assisted content (not even
disclosed), nor submission pull requests, commit messages, descriptions or
replies written by AI tools; other AI-generated material in the app has to be
disclosed. So the manifest and `cargo-sources.json` live only in Flathub's
repository, not here.

## What the game needs in the sandbox

What a build of the freedesktop runtime 26.08 showed:

- **Network**: online play, the shop's pages and local network games.
- **The window**: the game speaks X11 only (miniquad's Wayland backend isn't
  used), so under Wayland it runs through Xwayland and needs X11 itself,
  not just as a fallback; X11 also wants IPC (shared memory).
- **OpenGL**: the GPU devices. The runtime's Mesa draws it (llvmpipe without
  a GPU).
- **Sound**: the game plays through ALSA, and the runtime sends ALSA's
  default device to PulseAudio (or PipeWire's PulseAudio server), so the
  PulseAudio socket is enough: in a test the game's clicks reached the
  sound server as "ALSA plug-in [snook-legend]".
- **Game controllers** (optional): the game reads `/dev/input/js*`, which
  takes `--device=all` (`--device=input` from Flatpak 1.15.6); without it
  the game plays with the keyboard and the mouse only.
- Nothing else: the settings are kept in the sandbox's `XDG_CONFIG_HOME`
  (`~/.var/app/app.snooklegend.SnookLegend/config/snook-legend`), links open
  through the desktop's portal (the runtime's `xdg-open`), and notifications
  go through the notification portal (the runtime's `notify-send` uses it by
  itself in a sandbox).
- The build needs no network once the crates are downloaded; miniquad comes
  from the repository (`vendor/miniquad`, a `[patch.crates-io]` path), not
  from crates.io.

A copy installed before from the `.tar.xz` keeps its own settings in
`~/.config/snook-legend`, which the Flatpak can't see; what is kept in the
account comes back on signing in.

## Updates

The Flatpak never updates itself. In a sandbox (`/.flatpak-info`,
`FLATPAK_ID`) the game's updater is off (`crates/client/src/update.rs`,
`How::Flatpak`): Settings → Game says "Flathub keeps the game up to date",
there is no "Update automatically", and when the server asks for a newer
version the button opens the game's page on Flathub. The software center
(or `flatpak update`) brings new versions.

## Domain verification

Flathub verifies `snooklegend.app` through
`https://snooklegend.app/.well-known/org.flathub.VerifiedApps.txt`
(`public/.well-known/` in [snooklegend.app](https://github.com/allansomensi/snooklegend.app)).
The token is shown on flathub.org (Developer Portal → the app → Verification
→ Website) once the submission is accepted: add it there on a line of its own.
