//! `benilla-procgen` — deterministic procedural generation for Project B.
//!
//! Three product surfaces, one design rule: every artifact is a pure function of
//! `(generator_version, spec, seed)`. Same inputs, byte-identical outputs — which is what
//! replay, persistence, and later server validation all consume (the system review's rule:
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
//!   archetype and depth, entrance-safe NPC placement, exactly-one-boss constraint.
//!
//! Boundaries: this crate emits **data only**. It knows nothing about Bevy, MPQs, ADT
//! authoring, the WM bridge, or the server — integration mapping lives in
//! `docs/procgen/design.md`, and each consumer owns its own seam.

/// Generator identity stamped on every output. Bump on any change that can alter outputs
/// for the same `(spec, seed)` — this is the code half of the provenance pair (the seed is
/// the other half).
pub const GENERATOR_VERSION: &str = "procgen-v1";

pub mod dungeon;
pub mod item;
pub mod rng;
pub mod spawns;
pub mod terrain;

use dungeon::{Dungeon, DungeonSpec};
use item::{GeneratedItem, ItemRequest};
use spawns::{Spawn, SpawnSpec};
use terrain::{TerrainField, TerrainSpec};

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

/// Everything one run produced, with provenance.
#[derive(Clone, Debug, PartialEq)]
pub struct ProcgenOutput {
    pub generator_version: &'static str,
    pub seed: u64,
    pub items: Vec<GeneratedItem>,
    pub dungeon: Dungeon,
    pub terrain: TerrainField,
    pub spawns: Vec<Spawn>,
    /// FNV-1a over the version, seed, and every artifact field that matters for persistence.
    /// Two runs with the same spec and seed produce the same hash; any output-affecting code
    /// change must bump [`GENERATOR_VERSION`] (changing the hash for identical seeds, which
    /// is exactly the point).
    pub manifest_hash: u64,
}

/// Generate one complete, deterministic world-slice from `(spec, seed)`.
pub fn generate(spec: &ProcgenSpec) -> ProcgenOutput {
    // Domain-forked substreams: rolling more items cannot alter the dungeon, terrain, or
    // spawns, and vice versa — each subsystem sees only its own fork of the master seed.
    let master = rng::ProcRng::new(spec.seed);
    let items = if spec.items > 0 {
        item::generate_items(&mut master.fork(item::DOMAIN), &ItemRequest { count: spec.items, ..Default::default() })
    } else {
        Vec::new()
    };
    let dungeon = dungeon::generate(&mut master.fork(dungeon::DOMAIN), &spec.dungeon);
    let terrain = terrain::generate(&mut master.fork(terrain::DOMAIN), &spec.terrain);
    let spawns = spawns::populate(
        &mut master.fork(spawns::DOMAIN),
        &spec.spawns,
        &dungeon,
        &terrain,
    );

    let manifest_hash = manifest_hash(&items, &dungeon, &terrain, &spawns, spec.seed);
    ProcgenOutput {
        generator_version: GENERATOR_VERSION,
        seed: spec.seed,
        items,
        dungeon,
        terrain,
        spawns,
        manifest_hash,
    }
}

/// FNV-1a over the artifact contents. Explicit and boring on purpose: hashing `Debug`
/// strings would drift with formatting; this walks the numbers themselves.
fn manifest_hash(
    items: &[GeneratedItem],
    dungeon: &Dungeon,
    terrain: &TerrainField,
    spawns: &[Spawn],
    seed: u64,
) -> u64 {
    let mut h = fnv(GENERATOR_VERSION.as_bytes(), 0xcbf2_9ce4_8422_2325);
    h = fnv(&seed.to_le_bytes(), h);
    for item in items {
        h = fnv(&item.id.to_le_bytes(), h);
        h = fnv(&(item.stats.len() as u64).to_le_bytes(), h);
        for (stat, value) in &item.stats {
            h = fnv(&(*stat as u64).to_le_bytes(), h);
            h = fnv(&value.to_bits().to_le_bytes(), h);
        }
    }
    h = fnv(&(dungeon.width as u64).to_le_bytes(), h);
    h = fnv(&(dungeon.height as u64).to_le_bytes(), h);
    for tile in &dungeon.tiles {
        h = fnv(&[tile.discriminant_byte()], h);
    }
    h = fnv(&(terrain.width as u64).to_le_bytes(), h);
    h = fnv(&(terrain.height as u64).to_le_bytes(), h);
    for height in &terrain.heights {
        h = fnv(&(height.to_bits() as u32).to_le_bytes(), h);
    }
    for biome in &terrain.biomes {
        h = fnv(&[biome.discriminant_byte()], h);
    }
    for spawn in spawns {
        h = fnv(&spawn.position.0.to_le_bytes(), h);
        h = fnv(&spawn.position.1.to_le_bytes(), h);
        h = fnv(&[spawn.kind.discriminant_byte()], h);
        h = fnv(&spawn.level.to_le_bytes(), h);
    }
    h
}

fn fnv(bytes: &[u8], mut hash: u64) -> u64 {
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
