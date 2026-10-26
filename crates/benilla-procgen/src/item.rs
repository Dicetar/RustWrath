//! Part-composed, rarity-tiered, level-scaled items (the Borderlands model, in WoW-shaped
//! clothes): every item is a body + grip + core + adornment + material, each part pulled
//! from a rarity-gated weighted pool, stats rolled from the composed part multipliers, and
//! the name assembled from a tier-appropriate prefix, the body's name, and an affix suffix.
//!
//! Every output carries the full part provenance, so an item can be reproduced or audited
//! from `(seed, stream position)` alone.

use crate::rng::ProcRng;
use crate::ValidationError;

/// The fork domain for the item stream (see [`ProcRng::fork`]).
pub const DOMAIN: u64 = 0x4954_454d_0000_0001; // "ITEM"

/// Which stat a number is. Deliberately closed and small: affixes the rest of the project
/// already understands in some form.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatId {
    Damage,
    Armor,
    Stamina,
    Power,
    CritChance,
    CritDamage,
    Haste,
    Spirit,
}

impl StatId {
    pub fn discriminant_byte(self) -> u8 {
        match self {
            StatId::Damage => 1,
            StatId::Armor => 2,
            StatId::Stamina => 3,
            StatId::Power => 4,
            StatId::CritChance => 5,
            StatId::CritDamage => 6,
            StatId::Haste => 7,
            StatId::Spirit => 8,
        }
    }
}

/// Rarity tiers with drop weights and a global quality scalar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Epic,
    Legendary,
}

impl Rarity {
    /// Drop weights (per 100): a healthy common floor, a legendary tail.
    pub fn weighted_table() -> [(Rarity, u32); 5] {
        [
            (Rarity::Common, 44),
            (Rarity::Uncommon, 27),
            (Rarity::Rare, 16),
            (Rarity::Epic, 9),
            (Rarity::Legendary, 4),
        ]
    }

    /// Multiplier applied to primary stats.
    fn scalar(self) -> f32 {
        match self {
            Rarity::Common => 1.0,
            Rarity::Uncommon => 1.15,
            Rarity::Rare => 1.35,
            Rarity::Epic => 1.6,
            Rarity::Legendary => 2.0,
        }
    }

    /// How many secondary affixes the tier carries.
    fn affix_count(self) -> usize {
        match self {
            Rarity::Common => 0,
            Rarity::Uncommon => 1,
            Rarity::Rare => 2,
            Rarity::Epic => 3,
            Rarity::Legendary => 4,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Rarity::Common => "Common",
            Rarity::Uncommon => "Uncommon",
            Rarity::Rare => "Rare",
            Rarity::Epic => "Epic",
            Rarity::Legendary => "Legendary",
        }
    }
}

/// The equipment slot an item occupies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Sword,
    Axe,
    Bow,
    Staff,
    Gun,
    Shield,
    Helm,
    Chest,
}

impl ItemKind {
    /// The stat a level-1 bare specimen of this kind is worth before parts and rarity.
    fn primary(self) -> StatId {
        match self {
            ItemKind::Shield | ItemKind::Helm | ItemKind::Chest => StatId::Armor,
            _ => StatId::Damage,
        }
    }
}

/// One part slot in the composition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartSlot {
    Body,
    Grip,
    Core,
    Adornment,
    Material,
}

/// A part definition: which slot it fills, its display name, a relative weight inside its
/// pool, the minimum rarity that can roll it, and flat stat bonuses it contributes.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub id: &'static str,
    pub slot: PartSlot,
    pub name: &'static str,
    pub weight: u32,
    pub min_rarity: Rarity,
    /// (stat, flat bonus at level 1). Scales with item level.
    pub bonuses: &'static [(StatId, f32)],
}

/// Static part pools: every generated item carries `'static` part references, so its full
/// composition is auditable without owning copies.
static BODIES: [Part; 5] = [
    Part { id: "blade_std", slot: PartSlot::Body, name: "Balanced Blade", weight: 40, min_rarity: Rarity::Common, bonuses: &[(StatId::Damage, 2.0)] },
    Part { id: "blade_heavy", slot: PartSlot::Body, name: "Heavy Blade", weight: 25, min_rarity: Rarity::Uncommon, bonuses: &[(StatId::Damage, 3.5), (StatId::Haste, -1.0)] },
    Part { id: "blade_fine", slot: PartSlot::Body, name: "Fine Blade", weight: 18, min_rarity: Rarity::Rare, bonuses: &[(StatId::Damage, 3.0), (StatId::CritChance, 1.0)] },
    Part { id: "blade_arcane", slot: PartSlot::Body, name: "Arcane Core", weight: 10, min_rarity: Rarity::Epic, bonuses: &[(StatId::Damage, 3.0), (StatId::Power, 2.0)] },
    Part { id: "blade_relic", slot: PartSlot::Body, name: "Relic Hilt", weight: 4, min_rarity: Rarity::Legendary, bonuses: &[(StatId::Damage, 4.5), (StatId::Power, 2.0), (StatId::CritDamage, 3.0)] },
];

static GRIPS: [Part; 4] = [
    Part { id: "grip_worn", slot: PartSlot::Grip, name: "Worn Grip", weight: 45, min_rarity: Rarity::Common, bonuses: &[] },
    Part { id: "grip_oak", slot: PartSlot::Grip, name: "Oak Handle", weight: 30, min_rarity: Rarity::Common, bonuses: &[(StatId::Stamina, 1.0)] },
    Part { id: "grip_iron", slot: PartSlot::Grip, name: "Ironbound Handle", weight: 15, min_rarity: Rarity::Rare, bonuses: &[(StatId::Stamina, 2.0), (StatId::Armor, 1.0)] },
    Part { id: "grip_dragon", slot: PartSlot::Grip, name: "Dragonscale Wrap", weight: 6, min_rarity: Rarity::Legendary, bonuses: &[(StatId::Stamina, 4.0), (StatId::Power, 2.0)] },
];

static CORES: [Part; 5] = [
    Part { id: "core_plain", slot: PartSlot::Core, name: "Plain Steel", weight: 40, min_rarity: Rarity::Common, bonuses: &[(StatId::Damage, 1.0)] },
    Part { id: "core_tempered", slot: PartSlot::Core, name: "Tempered Edge", weight: 28, min_rarity: Rarity::Uncommon, bonuses: &[(StatId::Damage, 2.0)] },
    Part { id: "core_runed", slot: PartSlot::Core, name: "Runed Focus", weight: 18, min_rarity: Rarity::Rare, bonuses: &[(StatId::Power, 2.0)] },
    Part { id: "core_void", slot: PartSlot::Core, name: "Void-Touched Core", weight: 8, min_rarity: Rarity::Epic, bonuses: &[(StatId::Damage, 2.0), (StatId::Power, 3.0)] },
    Part { id: "core_star", slot: PartSlot::Core, name: "Starforged Core", weight: 3, min_rarity: Rarity::Legendary, bonuses: &[(StatId::Damage, 4.0), (StatId::CritChance, 2.0), (StatId::CritDamage, 4.0)] },
];

static ADORNMENTS: [Part; 4] = [
    Part { id: "orn_none", slot: PartSlot::Adornment, name: "Unadorned", weight: 50, min_rarity: Rarity::Common, bonuses: &[] },
    Part { id: "orn_gem", slot: PartSlot::Adornment, name: "Set Gem", weight: 25, min_rarity: Rarity::Uncommon, bonuses: &[(StatId::Spirit, 1.0)] },
    Part { id: "orn_sigil", slot: PartSlot::Adornment, name: "Engraved Sigil", weight: 15, min_rarity: Rarity::Rare, bonuses: &[(StatId::Haste, 1.5)] },
    Part { id: "orn_eye", slot: PartSlot::Adornment, name: "Watcher's Eye", weight: 7, min_rarity: Rarity::Epic, bonuses: &[(StatId::CritChance, 2.0)] },
];

static MATERIALS: [Part; 5] = [
    Part { id: "mat_iron", slot: PartSlot::Material, name: "Iron", weight: 45, min_rarity: Rarity::Common, bonuses: &[(StatId::Armor, 0.5)] },
    Part { id: "mat_steel", slot: PartSlot::Material, name: "Steel", weight: 30, min_rarity: Rarity::Uncommon, bonuses: &[(StatId::Damage, 1.0)] },
    Part { id: "mat_mithril", slot: PartSlot::Material, name: "Mithril", weight: 15, min_rarity: Rarity::Rare, bonuses: &[(StatId::Damage, 1.5), (StatId::Haste, 1.0)] },
    Part { id: "mat_elementium", slot: PartSlot::Material, name: "Elementium", weight: 7, min_rarity: Rarity::Epic, bonuses: &[(StatId::Damage, 2.5), (StatId::Power, 1.5)] },
    Part { id: "mat_azerite", slot: PartSlot::Material, name: "Azerite-Laced", weight: 3, min_rarity: Rarity::Legendary, bonuses: &[(StatId::Damage, 3.0), (StatId::Power, 3.0), (StatId::Stamina, 3.0)] },
];

static CATALOG: [(PartSlot, &[Part]); 5] = [
    (PartSlot::Body, &BODIES),
    (PartSlot::Grip, &GRIPS),
    (PartSlot::Core, &CORES),
    (PartSlot::Adornment, &ADORNMENTS),
    (PartSlot::Material, &MATERIALS),
];

/// The default catalog: five slots, rarity-gated weighted pools.
pub fn default_catalog() -> &'static [(PartSlot, &'static [Part])] {
    &CATALOG
}

/// Prefixes by rarity floor; suffixes follow "of the {suffix}".
const PREFIXES: &[(&str, Rarity)] = &[
    ("Sturdy", Rarity::Common),
    ("Keen", Rarity::Common),
    ("Vigilant", Rarity::Uncommon),
    ("Sinister", Rarity::Uncommon),
    ("Stormforged", Rarity::Rare),
    ("Banesoothing", Rarity::Rare),
    ("Titanbound", Rarity::Epic),
    ("Worldbreaker", Rarity::Legendary),
    ("Godspliced", Rarity::Legendary),
];

const SUFFIXES: &[&str] = &[
    "Bear", "Whale", "Falcon", "Viper", "Ember", "Tempest", "Eclipse", "Void", "Dawn",
];

/// A request for a batch of items.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemRequest {
    pub count: u32,
    /// Center of the level band; rolls spread ±2 around it.
    pub level: u32,
}

impl Default for ItemRequest {
    fn default() -> Self {
        Self { count: 5, level: 10 }
    }
}

impl ItemRequest {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.count > 10_000 {
            return Err(ValidationError::new("items.count", "maximum 10000 items per batch"));
        }
        if !(1..=80).contains(&self.level) {
            return Err(ValidationError::new("items.level", "expected 1..=80"));
        }
        Ok(())
    }
}

/// One finished item: identity, name, composition, and final stats.
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedItem {
    /// Deterministic identity: the stream position and seed are baked in.
    pub id: u64,
    pub name: String,
    pub kind: ItemKind,
    pub rarity: Rarity,
    pub level: u32,
    /// (stat, value) — primary stat first, then accumulated part bonuses and affixes.
    pub stats: Vec<(StatId, f32)>,
    /// The parts that composed it, in slot order (provenance).
    pub parts: Vec<&'static Part>,
}

const KINDS: [ItemKind; 8] = [
    ItemKind::Sword, ItemKind::Axe, ItemKind::Bow, ItemKind::Staff,
    ItemKind::Gun, ItemKind::Shield, ItemKind::Helm, ItemKind::Chest,
];

/// Roll `request.count` items from one stream.
pub fn generate_items(rng: &mut ProcRng, request: &ItemRequest) -> Result<Vec<GeneratedItem>, ValidationError> {
    request.validate()?;
    let catalog = default_catalog();
    Ok((0..request.count)
        .map(|i| generate_one(rng, catalog, request.level, i as u64))
        .collect())
}

fn generate_one(rng: &mut ProcRng, catalog: &'static [(PartSlot, &'static [Part])], level: u32, index: u64) -> GeneratedItem {
    let kind = *rng.choose(&KINDS);
    let rarity = *rng.weighted(&Rarity::weighted_table());
    let level = (level as i64 + rng.int(-2, 2)).clamp(1, 80) as u32;

    // One part per slot from the rarity-eligible pool. The pick is by weighted *index* so
    // the returned reference borrows from the static pool: full provenance, no copies.
    let mut parts = Vec::new();
    for (slot, pool) in catalog {
        let eligible: Vec<(usize, u32)> = pool
            .iter()
            .enumerate()
            .filter(|(_, p)| p.slot == *slot && rarity >= p.min_rarity)
            .map(|(i, p)| (i, p.weight))
            .collect();
        let index = if eligible.is_empty() {
            // A tier can out-vault a slot's whole pool only if the catalog is malformed;
            // fall back to the deepest part rather than panicking.
            pool.len() - 1
        } else {
            *rng.weighted(&eligible)
        };
        parts.push(&pool[index]);
    }

    // Primary: level curve × rarity scalar, plus part bonuses scaled by level.
    let primary = kind.primary();
    let level_scale = 1.0 + (level - 1) as f32 * 0.35;
    let mut stats: Vec<(StatId, f32)> = Vec::new();
    let base_primary = match primary {
        StatId::Armor => 6.0 + level as f32 * 2.2,
        _ => 4.0 + level as f32 * 1.6,
    };
    stats.push((primary, (base_primary * rarity.scalar()).max(1.0)));

    // Part bonuses (flat at level 1, scaled by the same curve).
    for part in &parts {
        for (stat, bonus) in part.bonuses {
            accumulate(&mut stats, *stat, bonus * level_scale);
        }
    }

    // Secondary affixes: distinct stats beyond the primary, rolled in a rarity-scaled band.
    let pool: Vec<StatId> = [
        StatId::Stamina, StatId::Power, StatId::CritChance, StatId::CritDamage,
        StatId::Haste, StatId::Spirit,
    ]
    .into_iter()
    .filter(|s| *s != primary)
    .collect();
    let mut affixes: Vec<StatId> = Vec::new();
    for _ in 0..rarity.affix_count() {
        let candidates: Vec<StatId> = pool.iter().copied().filter(|s| !affixes.contains(s)).collect();
        if candidates.is_empty() {
            break;
        }
        affixes.push(*rng.choose(&candidates));
    }
    for stat in affixes {
        let roll = rng.range(0.6, 1.4) * level_scale * rarity.scalar();
        accumulate(&mut stats, stat, (roll * 10.0).round() / 10.0);
    }

    // Name: prefix (tier-appropriate) + body name + "of the" suffix.
    let body_name = parts[0].name;
    let eligible_prefixes: Vec<&&str> = PREFIXES
        .iter()
        .filter(|(_, r)| rarity >= *r)
        .map(|(p, _)| p)
        .collect();
    let prefix = rng.choose(&eligible_prefixes);
    let suffix = *rng.choose(SUFFIXES);
    let name = format!("{prefix} {body_name} of the {suffix}");

    // Deterministic id: mix the stream's rolling state so it is reproducible from (seed,
    // position) without depending on future rolls.
    let id = mix_id(rng.peek_state(), index);

    GeneratedItem { id, name, kind, rarity, level, stats, parts }
}

fn accumulate(stats: &mut Vec<(StatId, f32)>, stat: StatId, value: f32) {
    if let Some(entry) = stats.iter_mut().find(|(s, _)| *s == stat) {
        entry.1 += value;
    } else {
        stats.push((stat, value));
    }
}

fn mix_id(state: u64, index: u64) -> u64 {
    let z = state ^ index.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    (z ^ (z >> 31)).wrapping_mul(0x94D0_49BB_1331_11EB)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_request_same_items() {
        let mut a = ProcRng::new(99);
        let mut b = ProcRng::new(99);
        let req = ItemRequest { count: 8, level: 20 };
        assert_eq!(
            generate_items(&mut a, &req).expect("valid item request"),
            generate_items(&mut b, &req).expect("valid item request"),
        );
    }

    #[test]
    fn parts_respect_rarity_gates_and_fill_all_slots() {
        let mut rng = ProcRng::new(5);
        for _ in 0..200 {
            let item = generate_one(&mut rng, default_catalog(), 15, 0);
            assert_eq!(item.parts.len(), 5, "one part per slot");
            assert!(item.parts.iter().all(|p| item.rarity >= p.min_rarity));
            assert!(!item.name.is_empty());
            assert!(item.stats.iter().any(|(s, v)| *s == item.kind.primary() && *v > 0.0));
        }
    }

    #[test]
    fn level_scales_primary() {
        let low = generate_one(&mut ProcRng::new(1), default_catalog(), 1, 0);
        let high = generate_one(&mut ProcRng::new(1), default_catalog(), 60, 0);
        let low_primary = low.stats.iter().find(|(s, _)| *s == low.kind.primary()).unwrap().1;
        let high_primary = high.stats.iter().find(|(s, _)| *s == high.kind.primary()).unwrap().1;
        assert!(high_primary > low_primary * 5.0, "60 levels must out-scale 1");
    }

    #[test]
    fn legendary_rolls_exist() {
        // With 500 rolls at the 4% legendary weight, absence would be a bug in the weights.
        let mut rng = ProcRng::new(321);
        let items = generate_items(&mut rng, &ItemRequest { count: 500, level: 30 }).expect("valid item request");
        assert!(items.iter().any(|i| i.rarity == Rarity::Legendary));
        assert!(items.iter().all(|i| i.rarity.affix_count() <= 4));
    }
}
