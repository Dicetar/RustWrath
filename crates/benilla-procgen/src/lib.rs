//! `benilla-procgen` — deterministic procedural generation for Project B.
//!
//! Four product surfaces, one design rule: every artifact is a pure function of
//! `(generator_version, spec, seed)`. Repeated inputs on the same build reproduce outputs for
//! replay, persistence, and later server validation (the system review's rule:
//! "seed alone is insufficient if code/assets change"; hence [`GENERATOR_VERSION`] and the
//! manifest hash carried on every output).
//!
//! - [`item`]: part-composed, rarity-tiered, level-scaled items (Borderlands-style: body /
//!   grip / core / adornment / material parts, prefix+suffix naming, affix rolls).
//! - [`dungeon`]: room-and-corridor generation — BSP partition, carved tile map, guaranteed
//!   connectivity from the entrance, archetype assignment (entrance / normal / treasure /
//!   boss) by graph distance.
//! - [`terrain`]: seeded value-noise FBM height and moisture fields with biome mapping.
//! - [`spawns`]: encounter population over a dungeon or terrain field — enemy packs by room
//!   archetype and depth, entrance-safe NPC placement, one boss in multi-room dungeons.
//!
//! Boundaries: this crate emits **data only**. It has no Bevy/MPQ dependency, ADT author,
//! network transport or server mutation owner. Owned candidates, encounter proposals and
//! inert existing-owner WM bundles live in [`artifact`], [`planning`] and [`wm_draft`].
//! Integration acceptance remains the consumer's responsibility; see `docs/procgen/design.md`.

/// Generator identity stamped on every output. Bump on any change that can alter outputs
/// for the same `(spec, seed)` — this is the code half of the provenance pair (the seed is
/// the other half).
pub const GENERATOR_VERSION: &str = "procgen-v2";

pub mod dungeon;
pub mod artifact;
pub mod content;
pub mod content_allocation;
pub mod json;
pub mod planning;
pub mod wm_draft;
mod validation;
pub mod item;
pub mod rng;
pub mod spawns;
pub mod terrain;

use dungeon::{Dungeon, DungeonSpec};
use item::{GeneratedItem, ItemRequest};
use spawns::{Spawn, SpawnSpec};
use terrain::{TerrainField, TerrainSpec};
pub use dungeon::ValidationError;

/// One complete generation request.
#[derive(Clone, Debug, PartialEq)]
pub struct ProcgenSpec {
    pub seed: u64,
    /// How many items to roll; zero skips the item stream entirely.
    pub items: u32,
    pub dungeon: DungeonSpec,
    pub terrain: TerrainSpec,
    pub spawns: SpawnSpec,
}

impl Default for ProcgenSpec {
    fn default() -> Self {
        Self {
            seed: 0,
            items: 5,
            dungeon: DungeonSpec::default(),
            terrain: TerrainSpec::default(),
            spawns: SpawnSpec::default(),
        }
    }
}

impl ProcgenSpec {
    /// Validate every subsystem and its worst-case output budget before allocating artifacts.
    pub fn validate(&self) -> Result<(), ValidationError> {
        ItemRequest { count: self.items, ..Default::default() }.validate()?;
        self.dungeon.validate()?;
        self.terrain.validate()?;
        self.spawns.validate(
            self.dungeon.target_rooms as usize,
            u64::from(self.terrain.width) * u64::from(self.terrain.height),
        )
    }
}

/// Everything one run produced, with provenance.
#[derive(Clone, Debug, PartialEq)]
pub struct ProcgenOutput {
    pub generator_version: &'static str,
    pub seed: u64,
    pub items: Vec<GeneratedItem>,
    pub dungeon: Dungeon,
    pub terrain: TerrainField,
    pub spawns: Vec<Spawn>,
    /// FNV-1a over the version, full request and every artifact field, with explicit lengths.
    /// Two runs with the same spec and seed produce the same hash; any output-affecting code
    /// change must bump [`GENERATOR_VERSION`] (changing the hash for identical seeds, which
    /// is exactly the point).
    pub manifest_hash: u64,
}

/// Generate one complete, deterministic world-slice from `(spec, seed)`.
///
/// # Errors
/// Returns the failing subsystem's [`ValidationError`](dungeon::ValidationError) when a
/// spec is rejected before anything is generated.
pub fn generate(spec: &ProcgenSpec) -> Result<ProcgenOutput, ValidationError> {
    spec.validate()?;
    // Domain-forked substreams: rolling more items cannot alter the dungeon, terrain, or
    // spawns, and vice versa — each subsystem sees only its own fork of the master seed.
    let master = rng::ProcRng::new(spec.seed);
    let items = if spec.items > 0 {
        item::generate_items(&mut master.fork(item::DOMAIN), &ItemRequest { count: spec.items, ..Default::default() })?
    } else {
        Vec::new()
    };
    let dungeon = dungeon::generate(&mut master.fork(dungeon::DOMAIN), &spec.dungeon)?;
    let terrain = terrain::generate(&mut master.fork(terrain::DOMAIN), &spec.terrain)?;
    let spawns = spawns::populate(
        &mut master.fork(spawns::DOMAIN),
        &spec.spawns,
        &dungeon,
        &terrain,
    )?;

    let manifest_hash = manifest_hash(spec, &items, &dungeon, &terrain, &spawns);
    Ok(ProcgenOutput {
        generator_version: GENERATOR_VERSION,
        seed: spec.seed,
        items,
        dungeon,
        terrain,
        spawns,
        manifest_hash,
    })
}

/// Canonical v2 manifest: fixed field order, explicit enum tags, little-endian numbers,
/// length-prefixed UTF-8 strings and sequences, and IEEE float bits. Not a security digest.
fn manifest_hash(
    spec: &ProcgenSpec,
    items: &[GeneratedItem],
    dungeon: &Dungeon,
    terrain: &TerrainField,
    spawns: &[Spawn],
) -> u64 {
    let mut h = ManifestHash(0xcbf2_9ce4_8422_2325);
    h.string("benilla-procgen-manifest-v2");
    h.string(GENERATOR_VERSION);
    h.u64(spec.seed);
    h.u32(spec.items);
    h.u32(spec.dungeon.width);
    h.u32(spec.dungeon.height);
    h.u32(spec.dungeon.target_rooms);
    h.u32(spec.dungeon.min_room);
    h.u32(spec.dungeon.extra_loops);
    h.u32(spec.terrain.width);
    h.u32(spec.terrain.height);
    h.u32(spec.terrain.frequency.to_bits());
    h.u32(spec.terrain.octaves);
    h.u32(spec.terrain.sea_level.to_bits());
    h.u32(spec.spawns.base_level);
    h.u32(spec.spawns.packs_per_room);
    h.u32(spec.spawns.terrain_density_per_100);
    h.len(items.len());
    for item in items {
        h.u64(item.id);
        h.string(&item.name);
        h.byte(match item.kind {
            item::ItemKind::Sword => 1,
            item::ItemKind::Axe => 2,
            item::ItemKind::Bow => 3,
            item::ItemKind::Staff => 4,
            item::ItemKind::Gun => 5,
            item::ItemKind::Shield => 6,
            item::ItemKind::Helm => 7,
            item::ItemKind::Chest => 8,
        });
        h.rarity(item.rarity);
        h.u32(item.level);
        h.stats(&item.stats);
        h.len(item.parts.len());
        for part in &item.parts {
            h.string(part.id);
            h.byte(match part.slot {
                item::PartSlot::Body => 1,
                item::PartSlot::Grip => 2,
                item::PartSlot::Core => 3,
                item::PartSlot::Adornment => 4,
                item::PartSlot::Material => 5,
            });
            h.string(part.name);
            h.u32(part.weight);
            h.rarity(part.min_rarity);
            h.stats(part.bonuses);
        }
    }
    h.u32(dungeon.width);
    h.u32(dungeon.height);
    h.len(dungeon.tiles.len());
    for tile in &dungeon.tiles {
        h.byte(tile.discriminant_byte());
    }
    h.len(dungeon.rooms.len());
    for room in &dungeon.rooms {
        h.len(room.id);
        h.u32(room.x);
        h.u32(room.y);
        h.u32(room.w);
        h.u32(room.h);
        h.byte(match room.archetype {
            dungeon::RoomArchetype::Entrance => 1,
            dungeon::RoomArchetype::Normal => 2,
            dungeon::RoomArchetype::Treasure => 3,
            dungeon::RoomArchetype::Boss => 4,
        });
        h.len(room.connections.len());
        for &next in &room.connections {
            h.len(next);
        }
    }
    h.len(dungeon.entrance);
    h.len(dungeon.boss_room);
    h.u32(terrain.width);
    h.u32(terrain.height);
    h.floats(&terrain.heights);
    h.floats(&terrain.moisture);
    h.len(terrain.biomes.len());
    for biome in &terrain.biomes {
        h.byte(biome.discriminant_byte());
    }
    h.len(spawns.len());
    for spawn in spawns {
        h.byte(match spawn.domain {
            spawns::SpawnDomain::Dungeon => 1,
            spawns::SpawnDomain::Overworld => 2,
        });
        h.u32(spawn.position.0);
        h.u32(spawn.position.1);
        match &spawn.kind {
            spawns::SpawnKind::Enemy { archetype, boss } => {
                h.byte(1);
                h.string(archetype);
                h.byte(u8::from(*boss));
            }
            spawns::SpawnKind::Npc { role } => {
                h.byte(2);
                h.byte(match role {
                    spawns::NpcRole::Vendor => 1,
                    spawns::NpcRole::QuestGiver => 2,
                    spawns::NpcRole::Innkeeper => 3,
                });
            }
        }
        h.u32(spawn.level);
        h.u32(spawn.encounter);
    }
    h.0
}

struct ManifestHash(u64);

impl ManifestHash {
    fn bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn byte(&mut self, value: u8) {
        self.bytes(&[value]);
    }

    fn u32(&mut self, value: u32) {
        self.bytes(&value.to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.bytes(&value.to_le_bytes());
    }

    fn len(&mut self, value: usize) {
        self.u64(value as u64);
    }

    fn string(&mut self, value: &str) {
        self.len(value.len());
        self.bytes(value.as_bytes());
    }

    fn floats(&mut self, values: &[f32]) {
        self.len(values.len());
        for value in values {
            self.u32(value.to_bits());
        }
    }

    fn stats(&mut self, values: &[(item::StatId, f32)]) {
        self.len(values.len());
        for (stat, value) in values {
            self.byte(stat.discriminant_byte());
            self.u32(value.to_bits());
        }
    }

    fn rarity(&mut self, value: item::Rarity) {
        self.byte(match value {
            item::Rarity::Common => 1,
            item::Rarity::Uncommon => 2,
            item::Rarity::Rare => 3,
            item::Rarity::Epic => 4,
            item::Rarity::Legendary => 5,
        });
    }
}
