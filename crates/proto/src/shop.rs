//! The shop: cosmetic items (cloth, cue, chalk, balls, tables, halls…),
//! coin packs and the VIP subscription. Shared by the server (which owns the wallets and sells)
//! and the game (which shows and draws the items).
//!
//! Every category has a free default that everyone owns. Other items are
//! bought with coins, come with an achievement, or are part of VIP.

/// What an item changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    Cloth,
    Cue,
    Chalk,
    Balls,
    /// The frame around the player's flag and name card.
    Frame,
    /// The colour of the name in the lounge (VIP).
    Name,
    /// What happens when this player wins on the 8.
    Effect,
    /// The table itself: wood, rails, legs, castings and pockets (only the
    /// player who owns it sees it).
    Table,
    /// The room around the table (only its owner sees it).
    Hall,
    /// A reaction to send during a match (see [`crate::reactions`]): kept
    /// once bought, never "put on".
    Reaction,
}

impl Category {
    pub const ALL: [Category; 10] = [
        Category::Cloth,
        Category::Cue,
        Category::Chalk,
        Category::Balls,
        Category::Table,
        Category::Hall,
        Category::Frame,
        Category::Name,
        Category::Effect,
        Category::Reaction,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Category::Cloth => "cloth",
            Category::Cue => "cue",
            Category::Chalk => "chalk",
            Category::Balls => "balls",
            Category::Frame => "frame",
            Category::Name => "name",
            Category::Effect => "effect",
            Category::Table => "table",
            Category::Hall => "hall",
            Category::Reaction => "reaction",
        }
    }

    pub fn parse(s: &str) -> Option<Category> {
        Category::ALL.into_iter().find(|c| c.code() == s)
    }

    /// The item everyone has.
    pub fn default_item(self) -> &'static str {
        match self {
            Category::Cloth => "cloth_green",
            Category::Cue => "cue_classic",
            Category::Chalk => "chalk_blue",
            Category::Balls => "balls_classic",
            Category::Frame => "frame_none",
            Category::Name => "name_tier",
            Category::Effect => "effect_none",
            Category::Table => "table_classic",
            Category::Hall => "hall_pool",
            Category::Reaction => "reaction_thumbs_up",
        }
    }
}

/// The pattern of a cue's forearm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CueStyle {
    /// Plain wood with rings.
    Plain,
    /// Pointed inlays ("points") of the accent colour into the forearm.
    Points,
    /// A spiral wrap of the accent colour.
    Spiral,
    /// A woven carbon pattern.
    Weave,
    /// Flames rising from the butt, from the accent to the ring colour.
    Flame,
    /// Polished stone with veins of the accent colour.
    Marble,
    /// Rows of cut stones of the accent colour, set in the ring's metal.
    Gems,
    /// Overlapping scales, shaded from the butt to the accent colour.
    Scales,
    /// Polished metal with engraved bands of the accent colour.
    Chrome,
    /// A night sky: clouds of the accent colour and stars.
    Nebula,
    /// Scrolls of fine gold (the accent) wire laid over lacquer.
    Filigree,
    /// Mother-of-pearl ovals and diamonds of the accent colour, shimmering
    /// towards the ring colour.
    Inlay,
    /// Folded steel: wavy layers of the accent colour in the butt's.
    Damascus,
    /// Forked bolts of the accent colour, bright at their core.
    Lightning,
    /// Curtains of light, from the accent to the ring colour, over a night
    /// sky.
    Aurora,
    /// Dark cracked stone with veins of glowing accent colour.
    Lava,
    /// Ice facets catching the light, of the accent colour.
    Crystal,
    /// Bold diagonal stripes of the accent colour (a candy cane, a flag).
    Stripes,
    /// Confetti of the accent and the ring colours (and white) scattered
    /// over the butt colour.
    Confetti,
    /// Six-pointed snowflakes of the accent colour.
    Snowflake,
    /// A spider's web of the accent colour.
    Web,
    /// Strings of little triangular flags (the festas juninas' bunting).
    Bunting,
}

impl CueStyle {
    /// Every style (the shop's tests check each one is drawn).
    pub const ALL: [CueStyle; 22] = [
        CueStyle::Plain,
        CueStyle::Points,
        CueStyle::Spiral,
        CueStyle::Weave,
        CueStyle::Flame,
        CueStyle::Marble,
        CueStyle::Gems,
        CueStyle::Scales,
        CueStyle::Chrome,
        CueStyle::Nebula,
        CueStyle::Filigree,
        CueStyle::Inlay,
        CueStyle::Damascus,
        CueStyle::Lightning,
        CueStyle::Aurora,
        CueStyle::Lava,
        CueStyle::Crystal,
        CueStyle::Stripes,
        CueStyle::Confetti,
        CueStyle::Snowflake,
        CueStyle::Web,
        CueStyle::Bunting,
    ];
}

/// The finish of a ball set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BallFinish {
    /// Phenolic resin: deep colour, bright clear coat.
    Gloss,
    /// A pearly sheen that shifts with the light.
    Pearl,
    /// Brushed metal.
    Metal,
    /// Old ivory: soft and warm.
    Matte,
    /// Polished stone: fine veins through the colour.
    Marble,
    /// A night sky in each ball: specks of starlight under the clear coat.
    Galaxy,
    /// Lit from within, like neon.
    Glow,
    /// Cut glass: deep colour and a very bright clear coat.
    Crystal,
}

impl BallFinish {
    pub const ALL: [BallFinish; 8] = [
        BallFinish::Gloss,
        BallFinish::Pearl,
        BallFinish::Metal,
        BallFinish::Matte,
        BallFinish::Marble,
        BallFinish::Galaxy,
        BallFinish::Glow,
        BallFinish::Crystal,
    ];
}

/// What a winning 8-ball does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectKind {
    None,
    /// A burst of sparks from the pocket.
    Sparks,
    /// Confetti over the table.
    Confetti,
    /// Rings of light spreading over the cloth.
    Shockwave,
    /// Flames licking up from the pocket.
    Flames,
    /// A shower of coins.
    Coins,
    /// Fireworks above the table.
    Fireworks,
    /// Bolts of lightning striking the pocket.
    Lightning,
    /// Snow drifting down over the table.
    Snow,
    /// Hearts floating up from the pocket.
    Hearts,
    /// A swirl of light sucked into the pocket.
    Vortex,
    /// Shooting stars streaking out of the pocket.
    Stars,
    /// A golden crown rising from the pocket in a burst of rays (VIP's own).
    Crown,
}

impl EffectKind {
    pub const ALL: [EffectKind; 13] = [
        EffectKind::None,
        EffectKind::Sparks,
        EffectKind::Confetti,
        EffectKind::Shockwave,
        EffectKind::Flames,
        EffectKind::Coins,
        EffectKind::Fireworks,
        EffectKind::Lightning,
        EffectKind::Snow,
        EffectKind::Hearts,
        EffectKind::Vortex,
        EffectKind::Stars,
        EffectKind::Crown,
    ];
}

/// The design of a table (the game builds it from code).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TableStyle {
    /// Dark mahogany, turned legs, brass castings.
    Classic,
    /// A beach bar's table: light teak, square legs, chrome castings.
    Beach,
    /// An English club's: deep mahogany, carved legs, leather pocket nets.
    Club,
    /// Black lacquer and brushed aluminium on two slab legs.
    Modern,
    /// Black ebony with gold castings and inlays.
    Royal,
    /// Reclaimed oak rails on a black steel frame with riveted plates.
    Industrial,
    /// A pro tournament table: charcoal rails, block legs, chrome.
    Tournament,
    /// Figured walnut on barley-twist legs, brass and leather pockets.
    Heritage,
    /// Ivory lacquer and gold on fluted columns.
    Deco,
    /// White Carrara marble rails, gold castings, fluted columns.
    Marble,
    /// Black gloss on slab legs, with a magenta and cyan glow beneath.
    Neon,
    /// Woven carbon rails, red trim, slab legs on a light strip.
    Carbon,
    /// Dark walnut and polished copper on a riveted steel frame.
    Copper,
    /// Frosted white-blue lacquer and silver, a cold light beneath.
    Ice,
    /// Black volcanic stone cracked with a glow of molten orange.
    Volcano,
    /// A country barn's table: rough pine on square posts, iron castings.
    Rustic,
    /// Black lacquer and gold: the season champion's table.
    Champion,
}

impl TableStyle {
    pub const ALL: [TableStyle; 17] = [
        TableStyle::Classic,
        TableStyle::Beach,
        TableStyle::Club,
        TableStyle::Modern,
        TableStyle::Royal,
        TableStyle::Industrial,
        TableStyle::Tournament,
        TableStyle::Heritage,
        TableStyle::Deco,
        TableStyle::Marble,
        TableStyle::Neon,
        TableStyle::Carbon,
        TableStyle::Copper,
        TableStyle::Ice,
        TableStyle::Volcano,
        TableStyle::Rustic,
        TableStyle::Champion,
    ];
}

/// The room around the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HallStyle {
    /// The pool hall: panelling, the bar, cue racks, posters, neon.
    Pool,
    /// A beach bar at sunset: a deck, open windows, palms, string lights.
    Beach,
    /// An English club: panelling, bookshelves, leather sofas, a fireplace.
    Club,
    /// An industrial loft: brick, steel windows over the city, Edison bulbs.
    Loft,
    /// A mountain lodge: log walls, a stone fireplace, snowy peaks outside.
    Lodge,
    /// A neon arcade: a black mirror floor, neon tubes, arcade cabinets.
    Arcade,
    /// An art deco lounge: a marble chessboard floor, gold, velvet, mirrors.
    Deco,
    /// A penthouse at night: glass walls on the city, pale marble, oak.
    Penthouse,
    /// A professional tournament arena: the table alone in a spotlight,
    /// dark stands, banners and the scoreboard, TV lighting. Official
    /// tournaments' finals are played in it.
    Arena,
}

impl HallStyle {
    pub const ALL: [HallStyle; 9] = [
        HallStyle::Pool,
        HallStyle::Beach,
        HallStyle::Club,
        HallStyle::Loft,
        HallStyle::Lodge,
        HallStyle::Arcade,
        HallStyle::Deco,
        HallStyle::Penthouse,
        HallStyle::Arena,
    ];
}

/// How an item looks (the game draws it; colours are sRGB).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    /// Cloth: the base colour of the felt.
    Cloth([u8; 3]),
    /// Cue: butt, forearm accent and ring colours, the forearm pattern and
    /// the shaft (maple, or a dark carbon shaft on the finest cues).
    Cue {
        butt: [u8; 3],
        accent: [u8; 3],
        ring: [u8; 3],
        style: CueStyle,
        shaft: [u8; 3],
    },
    /// Chalk: the colour on the cue tip.
    Chalk([u8; 3]),
    /// Balls: a palette for the seven colours (1–7 / 9–15), the 8 and the
    /// cue ball, and the ivory of stripes and number circles.
    Balls {
        colors: [[u8; 3]; 7],
        eight: [u8; 3],
        ivory: [u8; 3],
        finish: BallFinish,
    },
    /// Frame: two colours (a gradient) and whether it shines.
    Frame {
        a: [u8; 3],
        b: [u8; 3],
        shine: bool,
    },
    /// Name: the colour of the name (`None` is the tier's colour), and a
    /// second colour for a gradient.
    Name(Option<([u8; 3], [u8; 3])>),
    Effect {
        kind: EffectKind,
        color: [u8; 3],
    },
    Table(TableStyle),
    Hall(HallStyle),
    /// A reaction: its code (see [`crate::reactions::REACTIONS`]).
    Reaction(&'static str),
}

/// How an item is obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Price {
    /// Everyone has it.
    Free,
    Coins(u32),
    /// Comes with this achievement.
    Achievement(&'static str),
    /// Only for VIP members (while they are VIP).
    Vip,
    /// Comes with the welcome pack (and is kept).
    Pack,
    /// Never sold: only won (a tournament, an event, a season, an auction,
    /// VIP's item of the month) or given by the staff, and kept for good.
    Earned,
}

/// How rare an item is: shown with its own colour in the shop and the
/// collection.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Rarity {
    Common,
    Rare,
    Epic,
    Legendary,
}

impl Rarity {
    pub const ALL: [Rarity; 4] = [Rarity::Common, Rarity::Rare, Rarity::Epic, Rarity::Legendary];

    pub fn code(self) -> &'static str {
        match self {
            Rarity::Common => "common",
            Rarity::Rare => "rare",
            Rarity::Epic => "epic",
            Rarity::Legendary => "legendary",
        }
    }

    /// English name (translated by the game).
    pub fn name(self) -> &'static str {
        match self {
            Rarity::Common => "Common",
            Rarity::Rare => "Rare",
            Rarity::Epic => "Epic",
            Rarity::Legendary => "Legendary",
        }
    }

    /// Its colour (sRGB).
    pub fn color(self) -> [u8; 3] {
        match self {
            Rarity::Common => [168, 176, 186],
            Rarity::Rare => [64, 148, 255],
            Rarity::Epic => [176, 92, 255],
            Rarity::Legendary => [255, 176, 32],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Item {
    pub code: &'static str,
    pub category: Category,
    /// English name (translated by the game).
    pub name: &'static str,
    pub price: Price,
    pub look: Look,
}

impl Item {
    /// Can be bought with coins (and so given to a friend).
    pub fn coins(&self) -> Option<u32> {
        match self.price {
            Price::Coins(n) => Some(n),
            _ => None,
        }
    }
}

const fn cloth(code: &'static str, name: &'static str, price: Price, c: [u8; 3]) -> Item {
    Item { code, category: Category::Cloth, name, price, look: Look::Cloth(c) }
}

const fn cue(
    code: &'static str,
    name: &'static str,
    price: Price,
    butt: [u8; 3],
    accent: [u8; 3],
    ring: [u8; 3],
    style: CueStyle,
) -> Item {
    Item {
        code,
        category: Category::Cue,
        name,
        price,
        look: Look::Cue { butt, accent, ring, style, shaft: MAPLE },
    }
}

/// A maple shaft.
pub const MAPLE: [u8; 3] = [222, 190, 135];
/// A carbon-fibre shaft.
pub const CARBON_SHAFT: [u8; 3] = [26, 27, 30];

/// A cue with a carbon shaft (the finest ones).
#[allow(clippy::too_many_arguments)]
const fn cue_carbon(
    code: &'static str,
    name: &'static str,
    price: Price,
    butt: [u8; 3],
    accent: [u8; 3],
    ring: [u8; 3],
    style: CueStyle,
) -> Item {
    Item {
        code,
        category: Category::Cue,
        name,
        price,
        look: Look::Cue { butt, accent, ring, style, shaft: CARBON_SHAFT },
    }
}

const fn chalk(code: &'static str, name: &'static str, price: Price, c: [u8; 3]) -> Item {
    Item { code, category: Category::Chalk, name, price, look: Look::Chalk(c) }
}

const fn balls(
    code: &'static str,
    name: &'static str,
    price: Price,
    colors: [[u8; 3]; 7],
    eight: [u8; 3],
    ivory: [u8; 3],
    finish: BallFinish,
) -> Item {
    Item {
        code,
        category: Category::Balls,
        name,
        price,
        look: Look::Balls { colors, eight, ivory, finish },
    }
}

const fn frame(
    code: &'static str,
    name: &'static str,
    price: Price,
    a: [u8; 3],
    b: [u8; 3],
    shine: bool,
) -> Item {
    Item { code, category: Category::Frame, name, price, look: Look::Frame { a, b, shine } }
}

const fn name_style(code: &'static str, name: &'static str, a: [u8; 3], b: [u8; 3]) -> Item {
    Item { code, category: Category::Name, name, price: Price::Vip, look: Look::Name(Some((a, b))) }
}

const fn effect(
    code: &'static str,
    name: &'static str,
    price: Price,
    kind: EffectKind,
    color: [u8; 3],
) -> Item {
    Item { code, category: Category::Effect, name, price, look: Look::Effect { kind, color } }
}

const fn table(code: &'static str, name: &'static str, price: Price, style: TableStyle) -> Item {
    Item { code, category: Category::Table, name, price, look: Look::Table(style) }
}

const fn hall(code: &'static str, name: &'static str, price: Price, style: HallStyle) -> Item {
    Item { code, category: Category::Hall, name, price, look: Look::Hall(style) }
}

/// The classic colours, 1 to 7 (the stripes repeat them). The 7 is brown,
/// not a darker red: under the lamps a maroon 7 read as the 3 (and the 15
/// as the 11).
const CLASSIC: [[u8; 3]; 7] = [
    [255, 204, 10],
    [18, 58, 205],
    [228, 28, 24],
    [108, 36, 168],
    [255, 118, 12],
    [12, 142, 58],
    [112, 62, 30],
];

const IVORY: [u8; 3] = [246, 242, 228];

use BallFinish::{Crystal as Cut, Galaxy, Glow, Marble as Veined};
use BallFinish::{Gloss, Matte, Metal, Pearl};
use CueStyle::{
    Aurora, Bunting, Chrome, Confetti, Crystal, Damascus, Filigree, Flame, Gems, Inlay, Lava,
    Lightning, Marble, Nebula, Plain, Points, Scales, Snowflake, Spiral, Stripes, Weave, Web,
};

/// Every item, in the order the shop shows them.
pub const ITEMS: &[Item] = &[
    // Cloth.
    cloth("cloth_green", "Tournament green", Price::Free, [24, 96, 48]),
    cloth("cloth_blue", "Electric blue", Price::Coins(1000), [16, 54, 112]),
    cloth("cloth_burgundy", "Burgundy", Price::Coins(1000), [76, 13, 22]),
    cloth("cloth_olive", "Olive", Price::Coins(1000), [62, 72, 30]),
    cloth("cloth_navy", "Navy", Price::Coins(1250), [14, 28, 64]),
    cloth("cloth_charcoal", "Charcoal", Price::Coins(1500), [32, 34, 38]),
    cloth("cloth_teal", "Teal", Price::Coins(1500), [10, 78, 80]),
    cloth("cloth_forest", "Forest", Price::Coins(1500), [18, 58, 34]),
    cloth("cloth_slate", "Slate grey", Price::Coins(1750), [58, 66, 74]),
    cloth("cloth_wine", "Red wine", Price::Coins(1750), [96, 18, 40]),
    cloth("cloth_purple", "Royal purple", Price::Coins(2250), [50, 22, 84]),
    cloth("cloth_camel", "Camel", Price::Coins(2250), [116, 84, 44]),
    cloth("cloth_orange", "Burnt orange", Price::Coins(2250), [150, 62, 18]),
    cloth("cloth_rose", "Dusty rose", Price::Coins(2500), [138, 70, 82]),
    cloth("cloth_sand", "Sand", Price::Coins(2500), [150, 128, 92]),
    cloth("cloth_ice", "Ice blue", Price::Coins(3000), [70, 118, 150]),
    cloth("cloth_lime", "Lime", Price::Coins(3000), [92, 138, 24]),
    cloth("cloth_black", "Black tie", Price::Coins(3750), [16, 16, 18]),
    cloth("cloth_cobalt", "Cobalt", Price::Coins(3750), [20, 64, 168]),
    cloth("cloth_midnight", "Midnight", Price::Vip, [12, 17, 40]),
    cloth("cloth_emerald", "Emerald", Price::Vip, [6, 110, 74]),
    cloth("cloth_champion", "Champion's red", Price::Achievement("tournament_win"), [110, 12, 16]),
    // The account level's own (never sold).
    cloth("cloth_level10", "Regular's teal", Price::Achievement("level_10"), [10, 84, 84]),
    cloth("cloth_level20", "Veteran's plum", Price::Achievement("level_20"), [70, 22, 74]),
    cloth("cloth_level40", "Old-timer's black", Price::Achievement("level_40"), [18, 18, 20]),
    // The leagues' own (never sold): reaching the league brings them.
    cloth(
        "cloth_league_expert",
        "Expert league blue",
        Price::Achievement("rating_1500"),
        [16, 50, 120],
    ),
    cloth(
        "cloth_league_master",
        "Master league violet",
        Price::Achievement("rating_1700"),
        [62, 30, 110],
    ),
    cloth(
        "cloth_league_legend",
        "Legend league gold",
        Price::Achievement("rating_2100"),
        [120, 92, 20],
    ),
    cloth("cloth_welcome", "Lagoon", Price::Pack, [14, 100, 112]),
    // The finest, and those only won.
    cloth("cloth_crimson", "Crimson velvet", Price::Coins(6000), [118, 10, 28]),
    cloth("cloth_eclipse", "Eclipse", Price::Coins(12_000), [28, 18, 52]),
    cloth("cloth_arena", "Arena blue", Price::Earned, [18, 76, 144]),
    cloth("cloth_millennium", "Millennium gold", Price::Achievement("wins_1000"), [112, 86, 22]),
    cloth("cloth_holly", "Holly", Price::Earned, [12, 66, 40]),
    // VIP's items of the month: one each month, kept for good.
    cloth("vip_month_2026_12", "Winter frost", Price::Earned, [52, 92, 128]),
    cloth("vip_month_2027_03", "Rainforest", Price::Earned, [8, 72, 58]),
    cloth("vip_month_2027_06", "Bonfire", Price::Earned, [112, 32, 18]),
    cloth("vip_month_2027_09", "Spring meadow", Price::Earned, [62, 108, 38]),
    // Cues.
    cue("cue_classic", "Classic", Price::Free, [35, 20, 12], [20, 20, 24], [240, 238, 228], Plain),
    cue(
        "cue_maple",
        "Natural maple",
        Price::Coins(750),
        [196, 150, 96],
        [120, 80, 40],
        [30, 24, 20],
        Plain,
    ),
    cue(
        "cue_rosewood",
        "Rosewood",
        Price::Coins(1250),
        [92, 34, 22],
        [40, 18, 12],
        [200, 170, 110],
        Points,
    ),
    cue(
        "cue_walnut",
        "Walnut",
        Price::Coins(1500),
        [70, 44, 26],
        [190, 160, 110],
        [210, 200, 180],
        Points,
    ),
    cue(
        "cue_ebony",
        "Ebony & silver",
        Price::Coins(2000),
        [14, 12, 12],
        [60, 60, 66],
        [200, 205, 215],
        Plain,
    ),
    cue(
        "cue_zebra",
        "Zebrawood",
        Price::Coins(2250),
        [150, 110, 60],
        [40, 26, 16],
        [230, 220, 200],
        Spiral,
    ),
    cue(
        "cue_bocote",
        "Bocote",
        Price::Coins(2500),
        [140, 96, 44],
        [30, 20, 12],
        [200, 160, 80],
        Points,
    ),
    cue(
        "cue_carbon",
        "Carbon",
        Price::Coins(3000),
        [30, 32, 36],
        [200, 30, 30],
        [30, 32, 36],
        Weave,
    ),
    cue(
        "cue_ocean",
        "Deep ocean",
        Price::Coins(3000),
        [16, 40, 70],
        [60, 170, 200],
        [220, 230, 240],
        Spiral,
    ),
    cue(
        "cue_cherry",
        "Cherry",
        Price::Coins(3000),
        [120, 26, 30],
        [240, 230, 210],
        [200, 170, 110],
        Points,
    ),
    cue(
        "cue_jade",
        "Jade",
        Price::Coins(3500),
        [24, 80, 60],
        [200, 230, 210],
        [190, 150, 70],
        Points,
    ),
    cue(
        "cue_ivory",
        "Ivory inlay",
        Price::Coins(3750),
        [235, 225, 200],
        [35, 20, 12],
        [190, 150, 70],
        Points,
    ),
    cue(
        "cue_tiger",
        "Tiger",
        Price::Coins(4000),
        [200, 120, 30],
        [20, 16, 12],
        [20, 16, 12],
        Spiral,
    ),
    cue(
        "cue_amethyst",
        "Amethyst",
        Price::Coins(4500),
        [60, 24, 90],
        [200, 170, 240],
        [220, 220, 230],
        Points,
    ),
    cue(
        "cue_arctic",
        "Arctic carbon",
        Price::Coins(5000),
        [210, 220, 228],
        [60, 140, 220],
        [40, 44, 50],
        Weave,
    ),
    cue(
        "cue_inferno",
        "Inferno",
        Price::Coins(6000),
        [24, 10, 8],
        [240, 90, 20],
        [250, 200, 60],
        Spiral,
    ),
    cue(
        "cue_welcome",
        "First break",
        Price::Pack,
        [30, 44, 52],
        [40, 190, 200],
        [235, 235, 228],
        Spiral,
    ),
    cue_carbon(
        "cue_phoenix",
        "Phoenix",
        Price::Coins(7500),
        [18, 10, 8],
        [255, 96, 16],
        [255, 214, 90],
        Flame,
    ),
    cue_carbon(
        "cue_carrara",
        "Carrara marble",
        Price::Coins(6500),
        [236, 234, 230],
        [92, 96, 108],
        [200, 204, 212],
        Marble,
    ),
    cue_carbon(
        "cue_dragon",
        "Dragon scale",
        Price::Coins(8000),
        [8, 46, 30],
        [70, 200, 120],
        [214, 176, 72],
        Scales,
    ),
    cue_carbon(
        "cue_chrome",
        "Chrome",
        Price::Coins(7000),
        [170, 176, 186],
        [24, 26, 30],
        [235, 238, 244],
        Chrome,
    ),
    cue_carbon(
        "cue_galaxy",
        "Galaxy",
        Price::Coins(9000),
        [14, 8, 34],
        [150, 70, 220],
        [190, 200, 255],
        Nebula,
    ),
    cue_carbon(
        "cue_royal",
        "Crown",
        Price::Vip,
        [16, 22, 70],
        [60, 120, 255],
        [236, 196, 96],
        Gems,
    ),
    cue_carbon(
        "cue_obsidian",
        "Obsidian gold",
        Price::Vip,
        [10, 10, 12],
        [226, 180, 74],
        [236, 200, 110],
        Marble,
    ),
    cue_carbon(
        "cue_ruby",
        "Ruby throne",
        Price::Vip,
        [30, 6, 10],
        [235, 30, 60],
        [236, 196, 96],
        Gems,
    ),
    cue_carbon(
        "cue_legend",
        "Legend",
        Price::Achievement("rating_1900"),
        [20, 14, 10],
        [230, 160, 40],
        [250, 216, 120],
        Flame,
    ),
    // The account level's own (never sold).
    cue(
        "cue_level30",
        "Old hand",
        Price::Achievement("level_30"),
        [58, 30, 16],
        [196, 150, 84],
        [230, 214, 180],
        Points,
    ),
    cue_carbon(
        "cue_level50",
        "Lifer",
        Price::Achievement("level_50"),
        [14, 16, 22],
        [90, 190, 230],
        [220, 236, 246],
        Weave,
    ),
    // Earned by beating the computer ladder (never sold).
    cue(
        "cue_ladder",
        "Tour champion",
        Price::Achievement("ladder_champion"),
        [36, 18, 52],
        [226, 186, 84],
        [250, 236, 190],
        Spiral,
    ),
    // The collector's cues: very dear, each a piece of craft.
    cue_carbon(
        "cue_filigree",
        "Gold filigree",
        Price::Coins(15_000),
        [12, 10, 12],
        [232, 186, 84],
        [246, 214, 130],
        Filigree,
    ),
    cue_carbon(
        "cue_thunder",
        "Thunderbolt",
        Price::Coins(18_000),
        [14, 16, 34],
        [120, 200, 255],
        [228, 236, 255],
        Lightning,
    ),
    cue_carbon(
        "cue_damascus",
        "Damascus steel",
        Price::Coins(20_000),
        [70, 74, 82],
        [168, 174, 186],
        [36, 38, 42],
        Damascus,
    ),
    cue_carbon(
        "cue_glacier",
        "Glacier",
        Price::Coins(22_000),
        [190, 222, 240],
        [92, 170, 228],
        [240, 248, 255],
        Crystal,
    ),
    cue_carbon(
        "cue_abalone",
        "Abalone",
        Price::Coins(25_000),
        [10, 30, 52],
        [120, 220, 210],
        [214, 150, 230],
        Inlay,
    ),
    cue_carbon(
        "cue_volcano",
        "Volcano",
        Price::Coins(30_000),
        [22, 16, 14],
        [255, 110, 20],
        [255, 210, 80],
        Lava,
    ),
    cue_carbon(
        "cue_aurora",
        "Aurora",
        Price::Coins(35_000),
        [6, 12, 30],
        [60, 240, 170],
        [190, 90, 255],
        Aurora,
    ),
    cue_carbon(
        "cue_emperor",
        "Emperor",
        Price::Coins(60_000),
        [44, 8, 66],
        [246, 202, 96],
        [250, 250, 255],
        Filigree,
    ),
    // The near impossible: they come with the hardest achievements.
    cue_carbon(
        "cue_untouchable",
        "Untouchable",
        Price::Achievement("streak_10"),
        [8, 8, 10],
        [255, 206, 70],
        [255, 244, 200],
        Lightning,
    ),
    cue_carbon(
        "cue_midas",
        "Midas",
        Price::Achievement("golden_break"),
        [226, 178, 70],
        [120, 70, 10],
        [255, 236, 170],
        Chrome,
    ),
    cue_carbon(
        "cue_runner",
        "Table runner",
        Price::Achievement("runout_10"),
        [30, 32, 40],
        [210, 70, 60],
        [230, 232, 240],
        Damascus,
    ),
    cue_carbon(
        "cue_empire",
        "Empire",
        Price::Achievement("tournament_win_10"),
        [96, 6, 18],
        [244, 200, 92],
        [255, 236, 170],
        Gems,
    ),
    cue_carbon(
        "cue_millennium",
        "Millennium",
        Price::Achievement("wins_1000"),
        [10, 6, 24],
        [255, 190, 90],
        [255, 240, 200],
        Nebula,
    ),
    cue_carbon(
        "cue_four_corners",
        "Four corners",
        Price::Achievement("secret_four_corners"),
        [226, 240, 250],
        [70, 150, 255],
        [200, 230, 255],
        Crystal,
    ),
    cue_carbon(
        "cue_comeback",
        "Back from the dead",
        Price::Achievement("secret_comeback"),
        [26, 20, 18],
        [120, 255, 90],
        [220, 255, 200],
        Lava,
    ),
    // Only won: the season's champion, and the events and cups the game
    // holds through the year.
    cue_carbon(
        "cue_season_champion",
        "Season champion",
        Price::Earned,
        [12, 20, 56],
        [250, 206, 90],
        [255, 246, 210],
        Filigree,
    ),
    cue(
        "cue_christmas",
        "Candy cane",
        Price::Earned,
        [246, 244, 240],
        [214, 24, 40],
        [40, 140, 70],
        Stripes,
    ),
    cue_carbon(
        "cue_new_year",
        "Countdown",
        Price::Earned,
        [10, 10, 14],
        [246, 206, 96],
        [214, 220, 232],
        Confetti,
    ),
    cue(
        "cue_carnival",
        "Carnival",
        Price::Earned,
        [36, 20, 90],
        [255, 60, 170],
        [255, 214, 40],
        Confetti,
    ),
    cue(
        "cue_easter",
        "Easter egg",
        Price::Earned,
        [214, 196, 240],
        [140, 220, 190],
        [255, 200, 220],
        Confetti,
    ),
    cue(
        "cue_festa_junina",
        "Festa junina",
        Price::Earned,
        [176, 128, 72],
        [230, 50, 60],
        [40, 120, 220],
        Bunting,
    ),
    cue_carbon(
        "cue_halloween",
        "Spider web",
        Price::Earned,
        [18, 14, 20],
        [255, 128, 20],
        [150, 60, 200],
        Web,
    ),
    cue_carbon(
        "cue_winter_cup",
        "Winter cup",
        Price::Earned,
        [20, 44, 92],
        [226, 240, 255],
        [180, 200, 230],
        Snowflake,
    ),
    cue_carbon(
        "cue_summer_cup",
        "Summer cup",
        Price::Earned,
        [40, 10, 30],
        [255, 170, 40],
        [255, 90, 120],
        Aurora,
    ),
    cue_carbon(
        "cue_anniversary",
        "Anniversary",
        Price::Earned,
        [236, 230, 218],
        [200, 160, 70],
        [150, 30, 50],
        Filigree,
    ),
    cue(
        "cue_world_cup",
        "World cup",
        Price::Earned,
        [16, 120, 60],
        [252, 214, 30],
        [30, 60, 170],
        Stripes,
    ),
    // VIP's cues of the month.
    cue_carbon(
        "vip_month_2027_01",
        "New dawn",
        Price::Earned,
        [30, 16, 50],
        [255, 190, 110],
        [255, 236, 200],
        Aurora,
    ),
    cue(
        "vip_month_2027_04",
        "Sakura",
        Price::Earned,
        [250, 220, 228],
        [226, 110, 150],
        [255, 246, 240],
        Inlay,
    ),
    cue_carbon(
        "vip_month_2027_07",
        "Tide",
        Price::Earned,
        [12, 50, 88],
        [100, 210, 230],
        [230, 246, 250],
        Damascus,
    ),
    cue_carbon(
        "vip_month_2027_10",
        "Witching hour",
        Price::Earned,
        [20, 8, 30],
        [140, 255, 120],
        [190, 120, 255],
        Lightning,
    ),
    // Chalk.
    chalk("chalk_blue", "Blue chalk", Price::Free, [40, 80, 150]),
    // The account level's own (never sold).
    chalk("chalk_level5", "Apprentice's chalk", Price::Achievement("level_5"), [46, 168, 150]),
    chalk("chalk_level15", "Journeyman's chalk", Price::Achievement("level_15"), [150, 96, 204]),
    // The Skilled league's own (never sold).
    chalk(
        "chalk_league_skilled",
        "Skilled league chalk",
        Price::Achievement("rating_1300"),
        [70, 200, 96],
    ),
    chalk("chalk_green", "Green chalk", Price::Coins(400), [40, 130, 70]),
    chalk("chalk_red", "Red chalk", Price::Coins(400), [170, 40, 40]),
    chalk("chalk_tan", "Tan chalk", Price::Coins(400), [180, 150, 100]),
    chalk("chalk_grey", "Grey chalk", Price::Coins(400), [120, 124, 130]),
    chalk("chalk_purple", "Purple chalk", Price::Coins(500), [110, 50, 160]),
    chalk("chalk_orange", "Orange chalk", Price::Coins(500), [220, 110, 30]),
    chalk("chalk_pink", "Pink chalk", Price::Coins(500), [220, 110, 160]),
    chalk("chalk_teal", "Teal chalk", Price::Coins(500), [30, 150, 150]),
    chalk("chalk_yellow", "Yellow chalk", Price::Coins(600), [230, 200, 50]),
    chalk("chalk_black", "Black chalk", Price::Coins(750), [28, 28, 32]),
    chalk("chalk_white", "White chalk", Price::Coins(750), [235, 235, 230]),
    chalk("chalk_gold", "Gold chalk", Price::Vip, [210, 170, 60]),
    chalk("chalk_silver", "Silver chalk", Price::Vip, [190, 196, 205]),
    chalk("chalk_diamond", "Diamond dust chalk", Price::Coins(2500), [214, 232, 246]),
    chalk("chalk_champion", "Champion's chalk", Price::Earned, [196, 30, 44]),
    // Balls.
    balls("balls_classic", "Classic", Price::Free, CLASSIC, [22, 22, 24], IVORY, Gloss),
    balls(
        "balls_tv",
        "TV set",
        Price::Coins(1750),
        [
            [255, 214, 0],
            [0, 90, 220],
            [235, 30, 30],
            [140, 50, 200],
            [255, 130, 0],
            [0, 160, 70],
            [118, 64, 28],
        ],
        [16, 16, 18],
        [252, 250, 244],
        Gloss,
    ),
    balls(
        "balls_vintage",
        "Vintage",
        Price::Coins(2250),
        [
            [214, 170, 40],
            [40, 64, 130],
            [170, 48, 34],
            [96, 50, 110],
            [200, 110, 40],
            [40, 106, 60],
            [96, 64, 38],
        ],
        [30, 26, 24],
        [236, 222, 190],
        Matte,
    ),
    balls(
        "balls_earth",
        "Earth tones",
        Price::Coins(2500),
        [
            [196, 160, 70],
            [60, 80, 100],
            [150, 70, 50],
            [100, 70, 90],
            [180, 110, 60],
            [80, 110, 70],
            [64, 42, 16],
        ],
        [36, 30, 26],
        [238, 228, 205],
        Matte,
    ),
    balls(
        "balls_pastel",
        "Pastel",
        Price::Coins(3000),
        [
            [250, 220, 110],
            [120, 160, 230],
            [240, 120, 120],
            [180, 140, 220],
            [250, 170, 110],
            [120, 200, 150],
            [148, 110, 88],
        ],
        [40, 40, 48],
        [250, 248, 240],
        Gloss,
    ),
    balls(
        "balls_ocean",
        "Ocean",
        Price::Coins(3000),
        [
            [120, 220, 230],
            [10, 50, 140],
            [0, 150, 180],
            [40, 80, 170],
            [80, 200, 210],
            [10, 110, 120],
            [20, 60, 100],
        ],
        [8, 20, 40],
        [240, 250, 252],
        Gloss,
    ),
    balls(
        "balls_sunset",
        "Sunset",
        Price::Coins(3250),
        [
            [255, 200, 60],
            [140, 40, 110],
            [240, 70, 50],
            [110, 30, 90],
            [255, 130, 40],
            [200, 90, 70],
            [160, 30, 50],
        ],
        [40, 14, 30],
        [255, 244, 228],
        Gloss,
    ),
    balls(
        "balls_forest",
        "Forest",
        Price::Coins(3250),
        [
            [200, 190, 80],
            [40, 90, 70],
            [158, 44, 32],
            [60, 72, 90],
            [170, 110, 40],
            [30, 110, 50],
            [104, 68, 36],
        ],
        [20, 26, 18],
        [236, 232, 214],
        Matte,
    ),
    balls(
        "balls_neon",
        "Neon",
        Price::Coins(3750),
        [
            [230, 255, 0],
            [0, 200, 255],
            [255, 30, 90],
            [190, 60, 255],
            [255, 150, 0],
            [0, 240, 120],
            [255, 60, 30],
        ],
        [10, 10, 12],
        [250, 250, 250],
        Gloss,
    ),
    balls(
        "balls_candy",
        "Candy",
        Price::Coins(3750),
        [
            [255, 230, 120],
            [120, 200, 255],
            [255, 110, 150],
            [210, 150, 255],
            [255, 180, 120],
            [140, 240, 180],
            [120, 72, 50],
        ],
        [90, 50, 110],
        [255, 252, 250],
        Pearl,
    ),
    balls(
        "balls_pearl",
        "Pearl",
        Price::Coins(5000),
        CLASSIC,
        [30, 30, 36],
        [248, 246, 240],
        Pearl,
    ),
    balls(
        "balls_mono",
        "Monochrome",
        Price::Coins(5000),
        [
            [230, 230, 230],
            [60, 60, 66],
            [180, 180, 186],
            [40, 40, 44],
            [210, 210, 214],
            [90, 90, 96],
            [140, 140, 146],
        ],
        [12, 12, 14],
        [250, 250, 250],
        Gloss,
    ),
    balls(
        "balls_chrome",
        "Chrome",
        Price::Coins(6250),
        [
            [230, 190, 60],
            [70, 110, 190],
            [200, 70, 60],
            [140, 90, 170],
            [220, 140, 70],
            [70, 160, 110],
            [140, 98, 62],
        ],
        [40, 42, 46],
        [222, 226, 232],
        Metal,
    ),
    balls(
        "balls_royal",
        "Jewels",
        Price::Vip,
        [
            [230, 180, 40],
            [20, 40, 140],
            [160, 10, 30],
            [80, 20, 120],
            [210, 100, 20],
            [10, 100, 60],
            [104, 62, 30],
        ],
        [16, 14, 20],
        [245, 236, 210],
        Pearl,
    ),
    balls(
        "balls_gold",
        "Gold rush",
        Price::Vip,
        [
            [240, 200, 70],
            [180, 140, 50],
            [210, 160, 60],
            [150, 110, 40],
            [230, 180, 80],
            [170, 130, 50],
            [130, 90, 30],
        ],
        [20, 16, 10],
        [250, 240, 210],
        Metal,
    ),
    balls(
        "balls_marble",
        "Marble",
        Price::Coins(9000),
        [
            [236, 196, 70],
            [40, 70, 150],
            [190, 40, 40],
            [110, 60, 150],
            [226, 120, 50],
            [40, 128, 90],
            [120, 84, 60],
        ],
        [26, 26, 30],
        [244, 240, 232],
        Veined,
    ),
    balls(
        "balls_glow",
        "Glow",
        Price::Coins(11_000),
        [
            [240, 255, 60],
            [40, 140, 255],
            [255, 40, 80],
            [200, 80, 255],
            [255, 160, 20],
            [20, 255, 150],
            [150, 96, 50],
        ],
        [16, 10, 28],
        [250, 250, 255],
        Glow,
    ),
    balls(
        "balls_galaxy",
        "Galaxy",
        Price::Coins(15_000),
        [
            [255, 210, 90],
            [40, 70, 220],
            [230, 40, 90],
            [140, 60, 230],
            [255, 130, 60],
            [30, 190, 170],
            [120, 70, 60],
        ],
        [10, 8, 24],
        [236, 236, 252],
        Galaxy,
    ),
    balls(
        "balls_crystal",
        "Crystal",
        Price::Coins(20_000),
        [
            [255, 214, 40],
            [20, 80, 230],
            [230, 20, 40],
            [130, 40, 200],
            [255, 120, 0],
            [0, 170, 90],
            [124, 72, 40],
        ],
        [20, 20, 30],
        [250, 252, 255],
        Cut,
    ),
    balls(
        "balls_ruby",
        "Crown jewels",
        Price::Achievement("tournament_win_10"),
        [
            [250, 200, 60],
            [16, 60, 170],
            [200, 10, 40],
            [100, 30, 160],
            [240, 120, 30],
            [10, 130, 80],
            [116, 70, 40],
        ],
        [30, 4, 12],
        [255, 244, 228],
        Cut,
    ),
    balls(
        "balls_champion",
        "Champion's gold",
        Price::Earned,
        [
            [250, 214, 90],
            [40, 80, 180],
            [210, 50, 40],
            [120, 70, 170],
            [236, 140, 50],
            [40, 140, 90],
            [128, 90, 50],
        ],
        [24, 18, 8],
        [252, 238, 200],
        Metal,
    ),
    balls(
        "balls_christmas",
        "Christmas baubles",
        Price::Earned,
        [
            [250, 206, 60],
            [30, 70, 170],
            [210, 20, 40],
            [130, 40, 140],
            [240, 130, 40],
            [20, 120, 60],
            [118, 76, 46],
        ],
        [20, 40, 28],
        [250, 246, 236],
        Pearl,
    ),
    balls(
        "balls_carnival",
        "Carnival",
        Price::Earned,
        [
            [255, 230, 0],
            [0, 120, 255],
            [255, 30, 120],
            [160, 50, 255],
            [255, 140, 0],
            [0, 220, 140],
            [140, 90, 50],
        ],
        [30, 10, 50],
        [255, 252, 245],
        Glow,
    ),
    balls(
        "balls_halloween",
        "Haunted",
        Price::Earned,
        [
            [240, 200, 40],
            [70, 50, 160],
            [200, 40, 30],
            [120, 40, 160],
            [255, 120, 10],
            [90, 170, 40],
            [110, 70, 40],
        ],
        [16, 10, 18],
        [232, 226, 210],
        Matte,
    ),
    balls(
        "balls_world_cup",
        "World cup",
        Price::Earned,
        [
            [255, 214, 0],
            [10, 60, 180],
            [220, 30, 40],
            [100, 50, 170],
            [255, 120, 20],
            [0, 140, 70],
            [116, 70, 34],
        ],
        [20, 20, 22],
        [255, 255, 255],
        Gloss,
    ),
    // Tables.
    table("table_classic", "Classic", Price::Free, TableStyle::Classic),
    table("table_beach", "Beach bar", Price::Coins(4000), TableStyle::Beach),
    table("table_modern", "Modern", Price::Coins(5000), TableStyle::Modern),
    table("table_club", "English club", Price::Coins(6250), TableStyle::Club),
    table("table_royal", "Ebony & gold", Price::Vip, TableStyle::Royal),
    table("table_industrial", "Steel & oak", Price::Coins(5500), TableStyle::Industrial),
    table("table_tournament", "Tournament", Price::Coins(7000), TableStyle::Tournament),
    table("table_heritage", "Victorian walnut", Price::Coins(8000), TableStyle::Heritage),
    table("table_deco", "Ivory deco", Price::Coins(8500), TableStyle::Deco),
    table("table_copper", "Copper works", Price::Coins(9000), TableStyle::Copper),
    table("table_carbon", "Carbon racer", Price::Coins(10_000), TableStyle::Carbon),
    table("table_neon", "Neon night", Price::Coins(12_000), TableStyle::Neon),
    table("table_marble", "Carrara marble", Price::Coins(15_000), TableStyle::Marble),
    table("table_volcano", "Volcano", Price::Achievement("secret_comeback"), TableStyle::Volcano),
    table("table_champion", "Champion's gold", Price::Earned, TableStyle::Champion),
    table("table_ice", "Ice palace", Price::Earned, TableStyle::Ice),
    table("table_rustic", "Country barn", Price::Earned, TableStyle::Rustic),
    // Halls.
    hall("hall_pool", "Pool hall", Price::Free, HallStyle::Pool),
    hall("hall_beach", "Beach bar", Price::Coins(4500), HallStyle::Beach),
    hall("hall_loft", "Industrial loft", Price::Coins(5500), HallStyle::Loft),
    hall("hall_club", "English club", Price::Coins(6250), HallStyle::Club),
    hall("hall_lodge", "Mountain lodge", Price::Coins(6500), HallStyle::Lodge),
    hall("hall_arcade", "Neon arcade", Price::Coins(7000), HallStyle::Arcade),
    hall("hall_deco", "Art deco lounge", Price::Coins(8000), HallStyle::Deco),
    hall("hall_penthouse", "Skyline penthouse", Price::Vip, HallStyle::Penthouse),
    hall("hall_arena", "Pro arena", Price::Earned, HallStyle::Arena),
    // Frames.
    frame("frame_none", "No frame", Price::Free, [0, 0, 0], [0, 0, 0], false),
    frame("frame_bronze", "Bronze", Price::Coins(750), [176, 112, 60], [110, 66, 34], false),
    frame("frame_silver", "Silver", Price::Coins(1500), [214, 218, 226], [130, 136, 146], false),
    frame("frame_gold", "Gold", Price::Coins(2500), [240, 200, 90], [170, 120, 40], true),
    frame("frame_felt", "Green felt", Price::Coins(1000), [40, 140, 80], [16, 70, 40], false),
    frame("frame_wood", "Mahogany", Price::Coins(1250), [130, 60, 34], [70, 30, 16], false),
    frame("frame_ice", "Ice", Price::Coins(2250), [190, 230, 250], [90, 160, 210], true),
    frame("frame_fire", "Fire", Price::Coins(3000), [255, 180, 40], [210, 40, 20], true),
    frame("frame_neon", "Neon", Price::Coins(3000), [0, 240, 220], [220, 40, 240], true),
    frame("frame_diamond", "Diamond", Price::Coins(6250), [230, 250, 255], [140, 200, 255], true),
    frame("frame_crown", "VIP crown", Price::Vip, [250, 214, 110], [190, 130, 40], true),
    frame("frame_amethyst", "Amethyst", Price::Vip, [200, 150, 255], [100, 40, 170], true),
    frame(
        "frame_champion",
        "Champion",
        Price::Achievement("tournament_win"),
        [250, 220, 120],
        [170, 20, 30],
        true,
    ),
    frame("frame_obsidian", "Obsidian", Price::Coins(5000), [70, 70, 84], [10, 10, 14], true),
    frame("frame_prism", "Prism", Price::Coins(9000), [120, 240, 255], [255, 90, 200], true),
    frame(
        "frame_legend",
        "Legend",
        Price::Achievement("rating_2100"),
        [255, 220, 120],
        [120, 40, 200],
        true,
    ),
    frame(
        "frame_ruby",
        "Ruby",
        Price::Achievement("streak_10"),
        [255, 120, 140],
        [150, 0, 30],
        true,
    ),
    frame(
        "frame_phantom",
        "Phantom",
        Price::Achievement("secret_last_stand"),
        [200, 255, 240],
        [40, 70, 90],
        true,
    ),
    frame("frame_season", "Season champion", Price::Earned, [255, 236, 160], [30, 50, 130], true),
    frame("frame_pro", "Pro tour", Price::Earned, [240, 244, 250], [30, 120, 220], true),
    // VIP's frames of the month.
    frame("vip_month_2027_02", "Carnival", Price::Earned, [255, 214, 40], [220, 20, 150], true),
    frame("vip_month_2027_05", "Rose garden", Price::Earned, [255, 190, 200], [40, 120, 70], true),
    frame("vip_month_2027_08", "Golden hour", Price::Earned, [255, 220, 120], [230, 90, 30], true),
    frame("vip_month_2027_11", "Ember", Price::Earned, [255, 160, 60], [50, 14, 10], true),
    // Name styles (VIP).
    Item {
        code: "name_tier",
        category: Category::Name,
        name: "Tier colour",
        price: Price::Free,
        look: Look::Name(None),
    },
    name_style("name_gold", "Gold", [252, 216, 120], [214, 160, 60]),
    name_style("name_rose", "Rose gold", [250, 196, 186], [214, 140, 160]),
    name_style("name_aqua", "Aqua", [140, 240, 240], [60, 170, 230]),
    name_style("name_violet", "Violet", [220, 170, 255], [150, 90, 240]),
    name_style("name_ember", "Ember", [255, 214, 100], [242, 140, 40]),
    name_style("name_mint", "Mint", [170, 250, 200], [70, 210, 150]),
    name_style("name_frost", "Frost", [240, 248, 255], [160, 200, 240]),
    // Winning effects.
    effect("effect_none", "No effect", Price::Free, EffectKind::None, [0, 0, 0]),
    effect("effect_sparks", "Sparks", Price::Coins(1250), EffectKind::Sparks, [255, 210, 120]),
    effect(
        "effect_confetti",
        "Confetti",
        Price::Coins(2000),
        EffectKind::Confetti,
        [255, 255, 255],
    ),
    effect(
        "effect_shockwave",
        "Shockwave",
        Price::Coins(2500),
        EffectKind::Shockwave,
        [120, 220, 255],
    ),
    effect("effect_coins", "Coin shower", Price::Coins(3000), EffectKind::Coins, [240, 200, 80]),
    effect("effect_flames", "Flames", Price::Coins(3750), EffectKind::Flames, [255, 120, 30]),
    effect("effect_fireworks", "Fireworks", Price::Vip, EffectKind::Fireworks, [255, 220, 140]),
    effect("effect_royal", "Royal sparks", Price::Vip, EffectKind::Sparks, [200, 150, 255]),
    effect("effect_hearts", "Hearts", Price::Coins(4000), EffectKind::Hearts, [255, 90, 140]),
    effect(
        "effect_lightning",
        "Thunderstorm",
        Price::Coins(6000),
        EffectKind::Lightning,
        [170, 200, 255],
    ),
    effect("effect_vortex", "Black hole", Price::Coins(12_000), EffectKind::Vortex, [170, 90, 255]),
    effect(
        "effect_stars",
        "Shooting stars",
        Price::Achievement("golden_break"),
        EffectKind::Stars,
        [255, 226, 140],
    ),
    effect("effect_snow", "Snowfall", Price::Earned, EffectKind::Snow, [230, 242, 255]),
    effect(
        "effect_champion",
        "Champion's fireworks",
        Price::Earned,
        EffectKind::Fireworks,
        [255, 196, 60],
    ),
    // VIP's own: no one else ever has it.
    effect("effect_crown", "Crown of glory", Price::Vip, EffectKind::Crown, [255, 208, 90]),
    // Reactions (the free ones, achievements' and VIP's are not sold).
    reaction("reaction_thumbs_up", "Thumbs up", Price::Free, "thumbs_up"),
    reaction("reaction_eyes", "Eyes", Price::Coins(300), "eyes"),
    reaction("reaction_sob", "Tears", Price::Coins(300), "sob"),
    reaction("reaction_huff", "Huff", Price::Coins(300), "huff"),
    reaction("reaction_please", "Please", Price::Coins(300), "please"),
    reaction("reaction_sleepy", "Sleepy", Price::Coins(300), "sleepy"),
    reaction("reaction_rofl", "Rolling with laughter", Price::Coins(400), "rofl"),
    reaction("reaction_scream", "Scream", Price::Coins(400), "scream"),
    reaction("reaction_no_look", "Can't look", Price::Coins(400), "no_look"),
    reaction("reaction_salute", "Salute", Price::Coins(400), "salute"),
    reaction("reaction_mind_blown", "Mind blown", Price::Coins(500), "mind_blown"),
    reaction("reaction_clover", "Lucky clover", Price::Coins(500), "clover"),
    reaction("reaction_popcorn", "Popcorn", Price::Coins(600), "popcorn"),
    reaction("reaction_ok_hand", "OK", Price::Coins(800), "ok_hand"),
    reaction("reaction_crossed_fingers", "Fingers crossed", Price::Coins(800), "crossed_fingers"),
    reaction("reaction_call_me", "Call me", Price::Coins(800), "call_me"),
    reaction("reaction_nerd", "Nerd", Price::Coins(900), "nerd"),
    reaction("reaction_money_face", "Money face", Price::Coins(1000), "money_face"),
    reaction("reaction_monocle", "Monocle", Price::Coins(1000), "monocle"),
    reaction("reaction_facepalm", "Facepalm", Price::Coins(1200), "facepalm"),
    reaction("reaction_shrug", "Shrug", Price::Coins(1200), "shrug"),
    reaction("reaction_raised_hands", "Hands up", Price::Coins(1200), "raised_hands"),
    reaction("reaction_melting", "Melting", Price::Coins(1500), "melting"),
    reaction("reaction_ghost", "Ghost", Price::Coins(1500), "ghost"),
    reaction("reaction_alien", "Alien", Price::Coins(1500), "alien"),
    reaction("reaction_clown", "Clown", Price::Coins(2000), "clown"),
    reaction("reaction_devil", "Little devil", Price::Coins(2000), "devil"),
    reaction("reaction_angel", "Angel", Price::Coins(2000), "angel"),
    reaction("reaction_bomb", "Bomb", Price::Coins(2500), "bomb"),
    reaction("reaction_money_bag", "Money bag", Price::Coins(3000), "money_bag"),
    reaction("reaction_crystal_ball", "Crystal ball", Price::Coins(4000), "crystal_ball"),
    reaction("reaction_tornado", "Tornado", Price::Coins(5000), "tornado"),
    reaction("reaction_volcano", "Volcano", Price::Coins(6000), "volcano"),
    reaction("reaction_dragon", "Dragon", Price::Coins(9000), "dragon"),
    // Only won (the events and cups through the year).
    reaction("reaction_santa", "Santa", Price::Earned, "santa"),
    reaction("reaction_snowflake", "Snowflake", Price::Earned, "snowflake"),
    reaction("reaction_sun", "Summer sun", Price::Earned, "sun"),
    reaction("reaction_rabbit", "Easter bunny", Price::Earned, "rabbit"),
    reaction("reaction_soccer", "Football", Price::Earned, "soccer"),
    reaction("reaction_balloon", "Balloon", Price::Earned, "balloon"),
    reaction("reaction_season_medal", "Season medal", Price::Earned, "season_medal"),
];

const fn reaction(code: &'static str, name: &'static str, price: Price, r: &'static str) -> Item {
    Item { code, category: Category::Reaction, name, price, look: Look::Reaction(r) }
}

pub fn item(code: &str) -> Option<&'static Item> {
    ITEMS.iter().find(|i| i.code == code)
}

/// A pack of coins sold for real money (the price is set on the payment
/// provider and shown by the server).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoinPack {
    pub code: &'static str,
    pub coins: u32,
    /// Extra coins included (shown as a bonus).
    pub bonus: u32,
}

pub const COIN_PACKS: &[CoinPack] = &[
    CoinPack { code: "coins_500", coins: 500, bonus: 0 },
    CoinPack { code: "coins_1200", coins: 1200, bonus: 100 },
    CoinPack { code: "coins_2800", coins: 2800, bonus: 400 },
    CoinPack { code: "coins_7000", coins: 7000, bonus: 1500 },
];

pub fn coin_pack(code: &str) -> Option<&'static CoinPack> {
    COIN_PACKS.iter().find(|p| p.code == code)
}

/// The VIP subscription's product code, and the coins each paid month brings.
pub const VIP_PLAN: &str = "vip_monthly";
pub const VIP_MONTHLY_COINS: u32 = 400;
/// A month of VIP given to a friend (a one-off payment, not a subscription).
pub const VIP_GIFT: &str = "vip_gift";
/// A month of VIP bought in the Microsoft Store: an add-on bought once,
/// with the month's coins. Nothing renews by itself: another month is
/// another purchase, added after the time left.
pub const VIP_MONTH: &str = "vip_month";
/// How long a month of VIP bought once (in the Store) lasts, in seconds.
pub const VIP_MONTH_SECS: i64 = 31 * 86_400;
/// How long a given VIP month lasts, in seconds.
pub const VIP_GIFT_SECS: i64 = VIP_MONTH_SECS;

/// The welcome pack: a one-off purchase, once per account, with two items
/// sold nowhere else and some coins.
pub const WELCOME_PACK: &str = "welcome_pack";
pub const WELCOME_ITEMS: [&str; 2] = ["cue_welcome", "cloth_welcome"];
pub const WELCOME_COINS: u32 = 600;

/// The player has the welcome pack already.
pub fn has_welcome_pack(owned: &[String]) -> bool {
    owned.iter().any(|o| o == WELCOME_ITEMS[0])
}

/// Online wins a cue needs for each of its levels (purely cosmetic marks:
/// rings near the joint, then inlays, then gold).
pub const CUE_LEVELS: [u32; 6] = [10, 25, 50, 100, 250, 500];
/// The highest level a cue reaches.
pub const MAX_CUE_LEVEL: u8 = CUE_LEVELS.len() as u8;

/// The level of a cue that has won `wins` games (0 to [`MAX_CUE_LEVEL`]).
pub fn cue_level(wins: u32) -> u8 {
    CUE_LEVELS.iter().filter(|&&n| wins >= n).count() as u8
}

/// Wins needed for the level after `wins` (`None` at the top level).
pub fn next_cue_level_at(wins: u32) -> Option<u32> {
    CUE_LEVELS.iter().copied().find(|&n| wins < n)
}

/// How far a cue is from its current level to the next, 0..=1 (1 at the top).
pub fn cue_level_progress(wins: u32) -> f32 {
    let level = cue_level(wins) as usize;
    let Some(next) = next_cue_level_at(wins) else { return 1.0 };
    let from = if level == 0 { 0 } else { CUE_LEVELS[level - 1] };
    (wins - from) as f32 / (next - from) as f32
}

/// Items rarer than their price says: the season champion's, the pro
/// arena's and VIP's own victory animation.
const LEGENDARY: &[&str] = &[
    "cue_season_champion",
    "table_champion",
    "frame_season",
    "balls_champion",
    "effect_champion",
    "reaction_season_medal",
    "hall_arena",
    "effect_crown",
];

/// How rare an item is. Items that are only won are at least epic; the
/// most expensive ones, the season champion's and those of the near
/// impossible achievements (rubies) are legendary; a secret's or a
/// diamond's are epic.
pub fn rarity(item: &Item) -> Rarity {
    if LEGENDARY.contains(&item.code) {
        return Rarity::Legendary;
    }
    match item.price {
        Price::Free | Price::Pack => Rarity::Common,
        Price::Coins(n) if n >= 10_000 => Rarity::Legendary,
        Price::Coins(n) if n >= 4_000 => Rarity::Epic,
        Price::Coins(n) if n >= 1_500 => Rarity::Rare,
        Price::Coins(_) => Rarity::Common,
        Price::Vip => Rarity::Epic,
        Price::Achievement(a) => achievement_rarity(a),
        Price::Earned => Rarity::Epic,
    }
}

/// How rare what comes with an achievement is (its medal: ruby is
/// legendary; diamond and the secrets epic; the rest rare).
pub fn achievement_rarity(code: &str) -> Rarity {
    match crate::ACHIEVEMENTS.iter().find(|a| a.code == code) {
        Some(a) if a.level >= crate::achievements::RUBY => Rarity::Legendary,
        Some(a) if a.level >= 3 || a.secret => Rarity::Epic,
        _ => Rarity::Rare,
    }
}

/// How rare a reaction is: its item's rarity, or its achievement's (VIP's
/// are epic, the free ones common).
pub fn reaction_rarity(r: &crate::Reaction) -> Rarity {
    if let Some(item) = r.item() {
        return rarity(item);
    }
    match r.unlock {
        None => Rarity::Common,
        Some(_) if r.is_vip() => Rarity::Epic,
        Some(a) => achievement_rarity(a),
    }
}

/// Whether an item is never sold, only won: the shop doesn't show it (the
/// collection does, with how it was won).
pub fn earned_only(item: &Item) -> bool {
    item.price == Price::Earned
}

/// Whether an item stays with the account that got it: it can't be given
/// to a friend nor put up for auction (achievements', champions' and other
/// items that are only won, so that they keep meaning something).
pub fn bound(item: &Item) -> bool {
    matches!(item.price, Price::Achievement(_) | Price::Earned | Price::Vip | Price::Free)
}

/// VIP's item of the month `month` (`YYYY-MM`): the item coded
/// `vip_month_YYYY_MM`, if the catalogue has one.
pub fn vip_month_item(month: &str) -> Option<&'static Item> {
    let code = format!("vip_month_{}", month.replace('-', "_"));
    ITEMS.iter().find(|i| i.code == code)
}

/// Whether a player owns or may use `item`.
pub fn can_use(item: &Item, owned: &[String], achievements: &[String], vip: bool) -> bool {
    match item.price {
        Price::Free => true,
        Price::Coins(_) | Price::Pack | Price::Earned => owned.iter().any(|o| o == item.code),
        Price::Achievement(a) => achievements.iter().any(|c| c == a),
        Price::Vip => vip,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How far apart two sRGB colours look (CIE76 ΔE: about 2 is the least
    /// difference seen side by side; balls in a rack need far more).
    fn delta_e(a: [u8; 3], b: [u8; 3]) -> f64 {
        let lab = |c: [u8; 3]| {
            let lin = |v: u8| {
                let v = v as f64 / 255.0;
                if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
            };
            let (r, g, b) = (lin(c[0]), lin(c[1]), lin(c[2]));
            let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047;
            let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883;
            let f = |t: f64| if t > 0.008856 { t.cbrt() } else { 7.787 * t + 16.0 / 116.0 };
            [116.0 * f(y) - 16.0, 500.0 * (f(x) - f(y)), 200.0 * (f(y) - f(z))]
        };
        let (p, q) = (lab(a), lab(b));
        ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
    }

    /// In every ball set but the ones made of shades of one colour, the 7
    /// (brown) is told apart from the 3 (red) at a glance: a maroon 7 that
    /// was a darker red read as the 3 on the table.
    #[test]
    fn the_seven_is_not_taken_for_the_three() {
        let tonal = ["balls_ocean", "balls_mono", "balls_gold", "balls_sunset"];
        for item in ITEMS.iter().filter(|i| !tonal.contains(&i.code)) {
            if let Look::Balls { colors, .. } = item.look {
                let d = delta_e(colors[2], colors[6]);
                assert!(d >= 30.0, "{}: the 3 and the 7 are {d:.0} apart", item.code);
            }
        }
    }

    #[test]
    fn catalogue_is_consistent() {
        let mut codes: Vec<_> = ITEMS.iter().map(|i| i.code).collect();
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), ITEMS.len());
        for c in Category::ALL {
            let d = item(c.default_item()).expect("default exists");
            assert_eq!(d.category, c);
            assert_eq!(d.price, Price::Free);
            assert!(d.code.starts_with(c.code()));
        }
        for i in ITEMS {
            let vip_month = i.code.starts_with("vip_month_");
            assert!(i.code.starts_with(i.category.code()) || vip_month, "{}", i.code);
            if let Price::Achievement(a) = i.price {
                assert!(crate::ACHIEVEMENTS.iter().any(|x| x.code == a), "{a}");
            }
        }
        for code in WELCOME_ITEMS {
            assert_eq!(item(code).map(|i| i.price), Some(Price::Pack), "{code}");
        }
        assert!(ITEMS.iter().filter(|i| i.price == Price::Pack).count() == WELCOME_ITEMS.len());
        let none: Vec<String> = Vec::new();
        assert!(can_use(item("cloth_green").unwrap(), &none, &none, false));
        assert!(!can_use(item("cloth_blue").unwrap(), &none, &none, false));
        assert!(can_use(item("cloth_blue").unwrap(), &["cloth_blue".into()], &none, false));
        assert!(can_use(item("cue_royal").unwrap(), &none, &none, true));
        // The ladder's reward: never for coins, only with its achievement.
        let reward = item(crate::ladder::REWARD_ITEM).unwrap();
        assert_eq!(reward.price, Price::Achievement(crate::ladder::ACHIEVEMENT));
        assert!(reward.coins().is_none());
        assert!(!can_use(reward, &[reward.code.to_string()], &none, true));
        assert!(can_use(reward, &none, &[crate::ladder::ACHIEVEMENT.to_string()], false));
        // Every category has items to sell besides its default.
        for c in Category::ALL {
            assert!(ITEMS.iter().filter(|i| i.category == c).count() > 1, "{c:?}");
            assert_eq!(Category::parse(c.code()), Some(c));
        }
        assert!(matches!(item("table_club").unwrap().look, Look::Table(TableStyle::Club)));
        assert!(matches!(item("hall_beach").unwrap().look, Look::Hall(HallStyle::Beach)));
        // Every reaction item names a reaction, under its own code.
        for i in ITEMS.iter().filter(|i| i.category == Category::Reaction) {
            let Look::Reaction(r) = i.look else { panic!("{}", i.code) };
            assert!(crate::reaction(r).is_some(), "{}", i.code);
            assert_eq!(i.code.strip_prefix(crate::reactions::ITEM_PREFIX), Some(r));
        }
    }

    /// Rarity, items only won and items bound to their owner.
    #[test]
    fn rarity_and_items_only_won() {
        for i in ITEMS {
            let r = rarity(i);
            match i.price {
                // Never sold, never given: at least epic, kept by who won it.
                Price::Earned => {
                    assert!(r >= Rarity::Epic, "{}", i.code);
                    assert!(bound(i) && i.coins().is_none() && earned_only(i), "{}", i.code);
                }
                Price::Achievement(a) => {
                    assert!(r >= Rarity::Rare && bound(i), "{}", i.code);
                    let level = crate::ACHIEVEMENTS.iter().find(|x| x.code == a).unwrap().level;
                    if level == crate::achievements::RUBY {
                        assert_eq!(r, Rarity::Legendary, "{}", i.code);
                    }
                }
                Price::Coins(_) => assert!(!bound(i) && !earned_only(i), "{}", i.code),
                Price::Free => assert_eq!(r, Rarity::Common, "{}", i.code),
                Price::Vip | Price::Pack => {}
            }
        }
        for code in LEGENDARY {
            assert_eq!(item(code).map(rarity), Some(Rarity::Legendary), "{code}");
        }
        // Something of every rarity among the cues and the balls; epic and
        // legendary tables and effects.
        for c in [Category::Cue, Category::Table, Category::Balls, Category::Effect] {
            for r in Rarity::ALL {
                let wanted = matches!(c, Category::Cue | Category::Balls) || r >= Rarity::Epic;
                let has = ITEMS.iter().any(|i| i.category == c && rarity(i) == r);
                assert!(has || !wanted, "{c:?} {r:?}");
            }
        }
        // The very dear cues are a real goal.
        let dear = ITEMS.iter().filter(|i| i.category == Category::Cue).filter_map(Item::coins);
        assert!(dear.filter(|n| *n >= 15_000).count() >= 5);
        assert!(item("cue_emperor").and_then(Item::coins) >= Some(60_000));
        // The champion's cue, and a VIP victory animation nobody else has.
        assert_eq!(item("cue_season_champion").unwrap().price, Price::Earned);
        let crown = item("effect_crown").unwrap();
        assert_eq!(crown.price, Price::Vip);
        assert!(matches!(crown.look, Look::Effect { kind: EffectKind::Crown, .. }));
        assert_eq!(
            ITEMS
                .iter()
                .filter(|i| matches!(i.look, Look::Effect { kind: EffectKind::Crown, .. }))
                .count(),
            1
        );
    }

    /// VIP's item of the month, December 2026 to November 2027: one each
    /// month, only won, a cloth, a cue and a frame in turn.
    #[test]
    fn vip_items_of_the_month() {
        let order = [Category::Cloth, Category::Cue, Category::Frame];
        for k in 0..12 {
            let (y, m) = if k == 0 { (2026, 12) } else { (2027, k) };
            let month = format!("{y}-{m:02}");
            let i = vip_month_item(&month).unwrap_or_else(|| panic!("no item for {month}"));
            assert_eq!(i.price, Price::Earned, "{month}");
            assert_eq!(i.category, order[k as usize % 3], "{month}");
        }
        assert!(vip_month_item("2026-11").is_none());
    }

    /// Every style, finish and kind the game can draw is worn by an item.
    #[test]
    fn every_look_is_used() {
        let looks: Vec<Look> = ITEMS.iter().map(|i| i.look).collect();
        for s in CueStyle::ALL {
            assert!(
                looks.iter().any(|l| matches!(l, Look::Cue { style, .. } if *style == s)),
                "{s:?}"
            );
        }
        for f in BallFinish::ALL {
            assert!(
                looks.iter().any(|l| matches!(l, Look::Balls { finish, .. } if *finish == f)),
                "{f:?}"
            );
        }
        for k in EffectKind::ALL {
            assert!(
                looks.iter().any(|l| matches!(l, Look::Effect { kind, .. } if *kind == k)),
                "{k:?}"
            );
        }
        for t in TableStyle::ALL {
            assert!(looks.contains(&Look::Table(t)), "{t:?}");
        }
        for h in HallStyle::ALL {
            assert!(looks.contains(&Look::Hall(h)), "{h:?}");
        }
    }

    #[test]
    fn cue_levels() {
        assert_eq!(cue_level(0), 0);
        assert_eq!(cue_level(9), 0);
        assert_eq!(cue_level(10), 1);
        assert_eq!(cue_level(24), 1);
        assert_eq!(cue_level(25), 2);
        assert_eq!(cue_level(57), 3);
        assert_eq!(cue_level(499), 5);
        assert_eq!(cue_level(500), MAX_CUE_LEVEL);
        assert_eq!(cue_level(u32::MAX), MAX_CUE_LEVEL);
        assert_eq!(next_cue_level_at(0), Some(10));
        assert_eq!(next_cue_level_at(57), Some(100));
        assert_eq!(next_cue_level_at(500), None);
        assert_eq!(cue_level_progress(0), 0.0);
        assert_eq!(cue_level_progress(5), 0.5);
        assert_eq!(cue_level_progress(75), 0.5, "halfway from 50 to 100");
        assert_eq!(cue_level_progress(900), 1.0);
        assert!(CUE_LEVELS.windows(2).all(|w| w[0] < w[1]));
    }
}
