//! Where this copy of the game was distributed, and what that changes.
//!
//! The same code makes every build. The Microsoft Store build (feature
//! `store`, see `docs/MSSTORE.md`) sells nothing through the game's own
//! payment pages: the Store's policies ask games to sell digital items
//! through the Store, so coin packs there are Store add-ons (see
//! `msstore.rs`), and VIP and the welcome pack are not offered.

/// This is the Microsoft Store build.
pub const STORE: bool = cfg!(feature = "store");

/// The Store build sells coin packs and months of VIP (as Store add-ons)
/// only when built with the feature `store-purchases`: without it the
/// game offers nothing for real money (coins are earned by playing), so
/// its age rating need not carry "in-game purchases". The release
/// workflow turns it on with the repository variable `MSSTORE_PURCHASES`
/// (see docs/MSSTORE.md); the server needs no change.
pub const STORE_PURCHASES: bool = cfg!(feature = "store-purchases");

/// Coin packs and VIP may be bought through the game's own payment pages
/// (Stripe); the Store build buys coin packs in the Store instead.
pub const fn real_money_purchases() -> bool {
    !STORE
}

/// The Store build offers its add-ons for sale (see [`STORE_PURCHASES`]).
pub const fn store_sales() -> bool {
    STORE && STORE_PURCHASES
}

/// The game's application ID: the Flatpak's name on Flathub, its desktop
/// file and its MetaInfo (`flatpak/` in the repository).
pub const APP_ID: &str = "app.snooklegend.SnookLegend";

/// The game's page on Flathub.
pub const FLATHUB_PAGE: &str = "https://flathub.org/apps/app.snooklegend.SnookLegend";

/// This copy runs as a Flatpak (installed from Flathub): the system's
/// software center, or `flatpak update`, brings new versions; the game
/// never replaces itself (its folder, `/app`, is read-only there anyway).
/// Flatpak puts `/.flatpak-info` in every sandbox and sets `FLATPAK_ID`;
/// the same executable outside one updates itself as before. Worked out
/// once.
pub fn flatpak() -> bool {
    static FLATPAK: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FLATPAK.get_or_init(|| {
        cfg!(target_os = "linux")
            && (std::path::Path::new("/.flatpak-info").is_file()
                || std::env::var_os("FLATPAK_ID").is_some_and(|id| !id.is_empty()))
    })
}

#[cfg(test)]
mod tests {
    use snook_legend_proto::{LAN_DISCOVERY_PORT, LAN_GAME_PORT};

    /// The Store package's manifest stays in step with the game: the
    /// executable's name, the local network ports it opens in the firewall,
    /// the pictures it names and the values the packing script fills in.
    #[test]
    fn store_manifest_matches_the_game() {
        let m = include_str!("../msix/AppxManifest.xml");
        assert!(m.contains(&format!("Executable=\"{}.exe\"", env!("CARGO_PKG_NAME"))));
        for port in [LAN_GAME_PORT, LAN_DISCOVERY_PORT] {
            assert!(
                m.contains(&format!("LocalPortMin=\"{port}\" LocalPortMax=\"{port}\"")),
                "port {port} is not opened in the manifest's firewall rules"
            );
        }
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("msix");
        let files: Vec<String> = std::fs::read_dir(dir.join("Assets"))
            .unwrap()
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .collect();
        for asset in m.split("Assets\\").skip(1).filter_map(|s| s.split('"').next()) {
            let asset = asset.split('<').next().unwrap_or(asset);
            // The file itself, or versions of it with qualifiers
            // (`Square44x44Logo.scale-200.png`), which resources.pri indexes.
            let (stem, ext) = asset.rsplit_once('.').unwrap();
            let found = files.iter().any(|f| {
                f == asset
                    || (f.starts_with(&format!("{stem}.")) && f.ends_with(&format!(".{ext}")))
            });
            assert!(found, "missing msix/Assets/{asset}");
        }
        // The small icon in every size the shell asks for, without the
        // accent-coloured plate behind it.
        for size in crate::icon::tests::MSIX_TARGET_SIZES {
            for form in ["", "_altform-unplated", "_altform-lightunplated"] {
                let name = format!("Square44x44Logo.targetsize-{size}{form}.png");
                assert!(files.contains(&name), "missing msix/Assets/{name}");
            }
        }
        for key in
            ["IDENTITY_NAME", "PUBLISHER", "PUBLISHER_DISPLAY_NAME", "DISPLAY_NAME", "VERSION"]
        {
            assert!(m.contains(&format!("{{{{{key}}}}}")), "{key} is filled in by make-msix.ps1");
        }
    }

    /// The desktop file and the MetaInfo (`flatpak/`, see flatpak/README.md)
    /// stay in step with the game: its ID, the executable's name and the
    /// window's class (the desktop finds the launcher and the icon by it).
    #[test]
    fn flatpak_files_match_the_game() {
        use super::{APP_ID, FLATHUB_PAGE};
        let exe = env!("CARGO_PKG_NAME");
        let desktop = include_str!("../../../flatpak/app.snooklegend.SnookLegend.desktop");
        for line in
            [format!("Exec={exe}"), format!("Icon={APP_ID}"), format!("StartupWMClass={APP_ID}")]
        {
            assert!(desktop.lines().any(|l| l == line), "{line} in the desktop file");
        }
        let metainfo = include_str!("../../../flatpak/app.snooklegend.SnookLegend.metainfo.xml");
        for tag in [
            format!("<id>{APP_ID}</id>"),
            format!("<launchable type=\"desktop-id\">{APP_ID}.desktop</launchable>"),
            format!("<binary>{exe}</binary>"),
        ] {
            assert!(metainfo.contains(&tag), "{tag} in the MetaInfo");
        }
        assert!(FLATHUB_PAGE.ends_with(&format!("/apps/{APP_ID}")));
    }
}
