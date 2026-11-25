//! Reactions: small pictures a player can send during a match. A few are
//! free; others come with achievements (including event ones), some are
//! sold in the shop for coins (see [`crate::shop`]: an item
//! `reaction_<code>` each), and a set is for VIP members while they are VIP.

/// A reaction. `art` names its picture in the game's atlas (a Twemoji code
/// point); `unlock` is the achievement that grants it (`None`: everyone,
/// [`VIP`]: VIP members, [`SHOP`]: bought in the shop).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reaction {
    pub code: &'static str,
    pub art: &'static str,
    pub unlock: Option<&'static str>,
}

/// The `unlock` of the reactions VIP members have while they are VIP.
pub const VIP: &str = "vip";
/// The `unlock` of the reactions sold in the shop.
pub const SHOP: &str = "shop";
/// The `unlock` of the reactions only won (an event, a cup, a season): an
/// item each, never sold, kept once won.
pub const EARNED: &str = "earned";

const fn r(code: &'static str, art: &'static str, unlock: Option<&'static str>) -> Reaction {
    Reaction { code, art, unlock }
}

/// Every reaction, in the order the picker shows them.
pub const REACTIONS: &[Reaction] = &[
    r("thumbs_up", "1f44d", None),
    r("clap", "1f44f", None),
    r("wow", "1f62e", None),
    r("handshake", "1f91d", None),
    // Everything else takes real play: rated games only.
    r("thinking", "1f914", Some("games_100")),
    r("phew", "1f605", Some("wins_50")),
    r("heart", "2764", Some("friend_10")),
    r("cool", "1f60e", Some("wins_100")),
    r("laugh", "1f602", Some("games_500")),
    r("ice", "1f976", Some("shutout_10")),
    r("eight", "1f3b1", Some("eight_100")),
    r("fire", "1f525", Some("streak_10")),
    r("target", "1f3af", Some("runout_10")),
    r("shark", "1f988", Some("wins_500")),
    r("muscle", "1f4aa", Some("games_1000")),
    r("trophy", "1f3c6", Some("tournament_win_5")),
    r("crown", "1f451", Some("rating_2100")),
    r("pumpkin", "1f383", Some("event_halloween")),
    r("tree", "1f384", Some("event_winter")),
    r("fireworks", "1f386", Some("event_new_year")),
    r("wave", "1f44b", Some("friend_1")),
    r("playful", "1f61c", Some("casual_25")),
    r("grin", "1f601", Some("wins_10")),
    r("bolt", "26a1", Some("streak_5")),
    r("broom", "1f9f9", Some("runout_1")),
    r("wall", "1f9f1", Some("shutout_1")),
    r("brain", "1f9e0", Some("rating_1500")),
    r("eagle", "1f985", Some("rating_1900")),
    r("skull", "1f480", Some("eight_250")),
    r("medal", "1f947", Some("tournament_win")),
    r("glow_star", "1f31f", Some("golden_break")),
    r("robot", "1f916", Some("ladder_champion")),
    r("cake", "1f382", Some("veteran_365")),
    r("goat", "1f410", Some("level_50")),
    r("lion", "1f981", Some("wins_1000")),
    r("unicorn", "1f984", Some("games_2500")),
    r("bear", "1f43b", Some("eight_500")),
    r("wolf", "1f43a", Some("streak_7")),
    r("tiger", "1f405", Some("runout_5")),
    r("skull_crossbones", "2620", Some("shutout_25")),
    r("sports_medal", "1f3c5", Some("tournament_win_10")),
    r("chess", "265f", Some("rating_1700")),
    r("hourglass", "231b", Some("veteran_730")),
    r("mask", "1f3ad", Some("event_carnival")),
    r("corn", "1f33d", Some("event_festa_junina")),
    // The secret achievements' (what they are for stays hidden until won).
    r("owl", "1f989", Some("secret_last_stand")),
    r("ninja", "1f977", Some("secret_steal")),
    r("zombie", "1f9df", Some("secret_comeback")),
    r("rainbow", "1f308", Some("secret_three_in_one")),
    r("comet", "2604", Some("secret_four_corners")),
    r("snail", "1f40c", Some("secret_marathon")),
    r("boxing", "1f94a", Some("secret_ugly")),
    r("magic_wand", "1fa84", Some("secret_spotted_eight")),
    r("thunder_cloud", "26c8", Some("secret_thunder_break")),
    // Sold in the shop, for coins.
    r("eyes", "1f440", Some(SHOP)),
    r("sob", "1f62d", Some(SHOP)),
    r("huff", "1f624", Some(SHOP)),
    r("please", "1f64f", Some(SHOP)),
    r("sleepy", "1f634", Some(SHOP)),
    r("rofl", "1f923", Some(SHOP)),
    r("scream", "1f631", Some(SHOP)),
    r("no_look", "1f648", Some(SHOP)),
    r("salute", "1fae1", Some(SHOP)),
    r("mind_blown", "1f92f", Some(SHOP)),
    r("clover", "1f340", Some(SHOP)),
    r("popcorn", "1f37f", Some(SHOP)),
    r("ok_hand", "1f44c", Some(SHOP)),
    r("crossed_fingers", "1f91e", Some(SHOP)),
    r("call_me", "1f919", Some(SHOP)),
    r("nerd", "1f913", Some(SHOP)),
    r("money_face", "1f911", Some(SHOP)),
    r("monocle", "1f9d0", Some(SHOP)),
    r("facepalm", "1f926", Some(SHOP)),
    r("shrug", "1f937", Some(SHOP)),
    r("raised_hands", "1f64c", Some(SHOP)),
    r("melting", "1fae0", Some(SHOP)),
    r("ghost", "1f47b", Some(SHOP)),
    r("alien", "1f47d", Some(SHOP)),
    r("clown", "1f921", Some(SHOP)),
    r("devil", "1f608", Some(SHOP)),
    r("angel", "1f607", Some(SHOP)),
    r("bomb", "1f4a3", Some(SHOP)),
    r("money_bag", "1f4b0", Some(SHOP)),
    r("crystal_ball", "1f52e", Some(SHOP)),
    r("tornado", "1f32a", Some(SHOP)),
    r("volcano", "1f30b", Some(SHOP)),
    r("dragon", "1f409", Some(SHOP)),
    // For VIP members, while they are VIP.
    r("star_eyes", "1f929", Some(VIP)),
    r("gem", "1f48e", Some(VIP)),
    r("rocket", "1f680", Some(VIP)),
    r("party", "1f389", Some(VIP)),
    r("hundred", "1f4af", Some(VIP)),
    r("sparkles", "2728", Some(VIP)),
    r("dizzy", "1f4ab", Some(VIP)),
    // Only won: an event, a cup, a season (items of their own).
    r("santa", "1f385", Some(EARNED)),
    r("snowflake", "2744", Some(EARNED)),
    r("sun", "2600", Some(EARNED)),
    r("rabbit", "1f430", Some(EARNED)),
    r("soccer", "26bd", Some(EARNED)),
    r("balloon", "1f388", Some(EARNED)),
    r("season_medal", "1f396", Some(EARNED)),
];

/// The reaction with this code.
pub fn reaction(code: &str) -> Option<&'static Reaction> {
    REACTIONS.iter().find(|r| r.code == code)
}

/// The prefix of the shop items that are reactions.
pub const ITEM_PREFIX: &str = "reaction_";

/// The reactions bought among a wallet's items (their codes).
pub fn bought(owned: &[String]) -> Vec<String> {
    owned.iter().filter_map(|i| i.strip_prefix(ITEM_PREFIX)).map(str::to_string).collect()
}

impl Reaction {
    /// Whether a player with these achievements may send it (VIP, shop and
    /// earned reactions: never; see [`Reaction::available`]).
    pub fn unlocked<S: AsRef<str>>(&self, achievements: &[S]) -> bool {
        !self.is_vip()
            && !self.is_item()
            && self.unlock.is_none_or(|a| achievements.iter().any(|c| c.as_ref() == a))
    }

    /// Whether a player with these achievements and these reactions bought
    /// (see [`bought`]), VIP or not, may send it.
    pub fn available<S: AsRef<str>, B: AsRef<str>>(
        &self,
        achievements: &[S],
        bought: &[B],
        vip: bool,
    ) -> bool {
        (vip && self.is_vip())
            || (self.is_item() && bought.iter().any(|b| b.as_ref() == self.code))
            || self.unlocked(achievements)
    }

    /// One of the reactions for VIP members.
    pub fn is_vip(&self) -> bool {
        self.unlock == Some(VIP)
    }

    /// One of the reactions sold in the shop.
    pub fn is_shop(&self) -> bool {
        self.unlock == Some(SHOP)
    }

    /// One of the reactions only won (an item, never sold).
    pub fn is_earned(&self) -> bool {
        self.unlock == Some(EARNED)
    }

    /// A reaction that is an item of the wallet (bought, or won).
    pub fn is_item(&self) -> bool {
        self.is_shop() || self.is_earned()
    }

    /// The achievement it comes with, if any.
    pub fn achievement(&self) -> Option<&'static crate::Achievement> {
        let code = self.unlock.filter(|_| !self.is_vip() && !self.is_item())?;
        crate::ACHIEVEMENTS.iter().find(|a| a.code == code)
    }

    /// Its item (a shop or an earned reaction).
    pub fn item(&self) -> Option<&'static crate::shop::Item> {
        if !self.is_item() {
            return None;
        }
        crate::shop::ITEMS.iter().find(|i| {
            i.code.strip_prefix(ITEM_PREFIX) == Some(self.code)
                && i.category == crate::shop::Category::Reaction
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_reaction_is_unique_and_reachable() {
        let mut codes: Vec<_> = REACTIONS.iter().map(|r| r.code).collect();
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), REACTIONS.len());
        for r in REACTIONS.iter().filter(|r| !r.is_vip() && !r.is_item()) {
            if let Some(a) = r.unlock {
                assert!(crate::ACHIEVEMENTS.iter().any(|x| x.code == a), "{a}");
            }
        }
        let none: [&str; 0] = [];
        assert!(reaction("thumbs_up").unwrap().unlocked(&none));
        assert!(!reaction("crown").unwrap().unlocked(&none));
        assert!(reaction("crown").unwrap().unlocked(&["rating_2100"]));
        let gem = reaction("gem").unwrap();
        assert!(gem.available(&none, &none, true) && !gem.available(&none, &none, false));
        assert!(!gem.unlocked(&[VIP]), "VIP comes from the wallet, not achievements");
        assert!(reaction("crown").unwrap().available(&["rating_2100"], &none, false));
        // Shop reactions: each an item for coins, usable once bought.
        let popcorn = reaction("popcorn").unwrap();
        assert!(!popcorn.available(&none, &none, true));
        assert!(!popcorn.unlocked(&[SHOP]));
        assert!(popcorn.available(&none, &["popcorn"], false));
        assert_eq!(bought(&["reaction_popcorn".into(), "cloth_blue".into()]), ["popcorn"]);
        let shop: Vec<_> = REACTIONS.iter().filter(|r| r.is_shop()).collect();
        assert!(shop.len() >= 6);
        for r in &shop {
            let item = r.item().unwrap_or_else(|| panic!("{} is not in the shop", r.code));
            assert!(item.coins().is_some(), "{}", r.code);
        }
        let items = crate::shop::ITEMS
            .iter()
            .filter(|i| i.category == crate::shop::Category::Reaction && i.coins().is_some())
            .count();
        assert_eq!(items, shop.len(), "every reaction item is a shop reaction");
        // Earned reactions: an item each, never sold, usable once won.
        let earned: Vec<_> = REACTIONS.iter().filter(|r| r.is_earned()).collect();
        assert!(earned.len() >= 5);
        for r in &earned {
            let item = r.item().unwrap_or_else(|| panic!("{} has no item", r.code));
            assert_eq!(item.price, crate::shop::Price::Earned, "{}", r.code);
            assert!(!r.available(&none, &none, true));
            assert!(r.available(&none, &[r.code], false));
            assert!(!r.unlocked(&[EARNED]));
        }
        let items = crate::shop::ITEMS
            .iter()
            .filter(|i| {
                i.category == crate::shop::Category::Reaction
                    && i.price == crate::shop::Price::Earned
            })
            .count();
        assert_eq!(items, earned.len());
        // Few are free.
        assert!(REACTIONS.iter().filter(|r| r.unlock.is_none()).count() <= 4);
    }
}
