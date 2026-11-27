# Releasing

The game is packaged and published by [dist](https://axodotdev.github.io/cargo-dist)
(cargo-dist). Its settings are in `dist-workspace.toml`; the workflow it
generates is `.github/workflows/release.yml` (don't edit that file by hand).

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

1. Set the version in `Cargo.toml` (`[workspace.package] version`), turn
   `CHANGELOG.md`'s "Not released yet" into a `## <version>` section (and
   start a new one empty after the release), and add one to "What's new"
   (`crates/client/whatsnew/*.md`), then write the Flatpak's releases from
   it (`python3 tools/gen_metainfo.py`; a test checks it was done).
2. Optional: the repository variable `SNOOK_LEGEND_SERVER` (Settings → Secrets and
   variables → Actions → Variables) is the game server baked into the
   release; without it the game uses the public server.
3. Commit, tag and push:

   ```bash
   git tag v0.2.0
   ```

   ```bash
   git push origin v0.2.0
   ```

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
