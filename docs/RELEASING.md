# Releasing

The game is packaged and published by [dist](https://axodotdev.github.io/cargo-dist)
(cargo-dist). Its settings are in `dist-workspace.toml`; the workflow it
generates is `.github/workflows/release.yml` (edited by hand only for its
`workflow_dispatch` trigger, which publish.yml needs: `dist generate` would
take it away; `allow-dirty` in dist-workspace.toml keeps dist from
complaining).

## What a release contains

| Platform | Files |
|---|---|
| Windows x64 | `snook-legend-x86_64-pc-windows-msvc.msi` (installer: Program Files, Start menu and optional desktop shortcut, uninstalls from "Installed apps"), `snook-legend-x86_64-pc-windows-msvc.zip` (portable) |
| macOS Apple Silicon | `snook-legend-aarch64-apple-darwin.tar.xz` |
| macOS Intel | `snook-legend-x86_64-apple-darwin.tar.xz` |
| Linux x64 | `snook-legend-x86_64-unknown-linux-gnu.tar.xz` |
| Linux ARM64 | `snook-legend-aarch64-unknown-linux-gnu.tar.xz` |
| All | `snook-legend-installer.sh` (macOS/Linux), `snook-legend-installer.ps1` (Windows), `sha256.sum`, `source.tar.gz` |

Every archive has the game, `README.md`, `LICENSE` and `CHANGELOG.md`. The
release notes come from the matching section of `CHANGELOG.md` (`## 0.2.0`).

The one-line installers download the right archive for the computer and put
`snook-legend` in `~/.local/bin` (or `$XDG_BIN_HOME`):

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/allansomensi/snook-legend/releases/latest/download/snook-legend-installer.sh | sh
```

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/allansomensi/snook-legend/releases/latest/download/snook-legend-installer.ps1 | iex"
```

The Microsoft Store gets its own package (MSIX) from the same tag, sent to
the Store with "What's new" in every language of its listing once the
repository variable `MSSTORE_APP_ID` is set; see [MSSTORE.md](MSSTORE.md). The Flatpak's desktop file and MetaInfo (for
Flathub) are in `flatpak/`; see [flatpak/README.md](../flatpak/README.md).

## Publishing a version

A version publishes itself once it reaches `main`: nothing is tagged by
hand.

1. Prepare the release in one commit: the version in `Cargo.toml`
   (`[workspace.package] version`, and `Cargo.lock` by building),
   `CHANGELOG.md`'s "Not released yet" turned into a `## <version>`
   section, a `## <version>` section in "What's new"
   (`crates/client/whatsnew/*.md`, every language, as many highlights in
   each), and the Flatpak's releases written from it
   (`python3 tools/gen_metainfo.py`). Check it:

   ```bash
   python3 tools/release_check.py
   ```

2. Push it to `main`. When **CI** has passed on it, **Publish a version**
   (`.github/workflows/publish.yml`) checks the release again
   (`tools/release_check.py`), tags the commit `v<version>` (annotated)
   and starts the release on that tag: **Release**, **Microsoft Store**
   (sent to the Store) and **Server image** (and its deploy on Render).

What keeps it safe:

- The tag is always the version in `Cargo.toml` of the commit it marks:
  nobody types it.
- Only the newest commit of `main`, only once CI has passed on it (every
  platform's tests, clippy, PostgreSQL, the server image, the advisories).
- Only a version newer than every one released, and never one tagged
  already: pushing more commits without raising the version publishes
  nothing.
- Only a release that is ready: the changelog, "What's new" in every
  language, the Store's notes and the MetaInfo all have the version
  (`tools/release_check.py` says what is missing, and nothing is tagged).
- One run at a time; a tag pushed by hand meanwhile wins, and nothing is
  started twice.

If a step fails, the run says why. Fix it and push again (a version not
tagged yet is published by the next commit that passes), or run
**Publish a version** by hand (Actions → Publish a version → Run workflow,
on `main`). If the tag was made but the release's workflows didn't start,
run it with "start again" ticked. A workflow of the release that failed on
its own is run again from its page, as always.

A tag pushed by hand still publishes a version, as before (the workflows
start on the tag's push):

```bash
git tag v0.2.0
```

```bash
git push origin v0.2.0
```

(Why the workflow starts the others itself: a tag pushed with a
workflow's own token sets off no other workflow, a rule of GitHub's to
keep workflows from setting each other off without end. Started on the
tag, they run exactly as on a pushed tag: `github.ref_name` is the tag.)

Optional: the repository variable `SNOOK_LEGEND_SERVER` (Settings → Secrets
and variables → Actions → Variables) is the game server baked into the
release; without it the game uses the public server.

The **Release** workflow builds every platform, makes the installers and
publishes the GitHub Release; the **Server image** workflow publishes
`ghcr.io/<owner>/snook-legend-server:<tag>`; the **Microsoft Store**
workflow packs the MSIX and sends it to the Store for certification. Flathub's copy is updated
by hand in its own repository (the tag, and `cargo-sources.json` when
`Cargo.lock` changed; see [flatpak/README.md](../flatpak/README.md)). A version with a suffix (`v0.2.0-beta.1`)
is published as a pre-release. Pull requests run `dist plan` only, which
checks the configuration.

## Trying it locally

```bash
cargo install cargo-dist --locked
```

```bash
dist plan
```

```bash
dist build --artifacts=all --target x86_64-unknown-linux-gnu
```

The files land in `target/distrib/`. The `.msi` can only be built on Windows
(WiX Toolset 3, present on GitHub's `windows-2022` runners).

## Changing the setup

- `dist-workspace.toml`: platforms, installers, where the installers put the
  game. Run `dist generate` afterwards to update `release.yml`.
- `.github/build-setup.yml`: steps run before each build (Linux libraries,
  `SNOOK_LEGEND_SERVER`). Run `dist generate` afterwards as well.
- `crates/client/wix/main.wxs`: the Windows installer (edited by hand; dist
  is told not to regenerate it with `allow-dirty = ["msi"]`). Keep the
  `UpgradeCode` as it is: it lets a new version replace the old one.

## The emblem

The logo is drawn by code in `crates/client/src/icon.rs` (window icon, menu,
lounge, loading screen). The files made from it live in
`crates/client/assets/icon/`: PNGs (64–512 px), `snook-legend.ico` (embedded in
`snook-legend.exe` by `crates/client/build.rs` and used by the installer) and the
installer's pictures (`wix-banner.bmp`, `wix-dialog.bmp`). After changing the
drawing, regenerate them:

```bash
cargo test --release -p snook-legend export_icons -- --ignored
```
