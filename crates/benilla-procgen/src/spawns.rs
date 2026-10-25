//! Encounter population: enemy packs and NPCs placed over a dungeon's rooms or a terrain
//! field's biomes. Constraints that make a layout *playable* rather than merely random:
//! the entrance is enemy-free, the boss room holds exactly one boss (with adds), treasure
//! rooms are guarded, and levels progress with distance from the entrance.

use crate::dungeon::{Dungeon, RoomArchetype};
use crate::rng::ProcRng;
use crate::terrain::{Biome, TerrainField};

/// The fork domain for the spawn stream.
pub const DOMAIN: u64 = 0x5350_4157_4e53_0004; // "SPAWNS"

/// What stands at a spawn point.
#[derive(Clone, Debug, PartialEq)]
pub enum SpawnKind {
    /// An enemy: pack archetype and level.
    Enemy { archetype: &'static str, boss: bool },
    /// A friendly: vendor / quest giver / flavor.
    Npc { role: NpcRole },
}

impl SpawnKind {
    pub fn discriminant_byte(&self) -> u8 {
        match self {
            SpawnKind::Enemy { boss: false, .. } => 1,
            SpawnKind::Enemy { boss: true, .. } => 2,
            SpawnKind::Npc { .. } => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NpcRole {
    Vendor,
    QuestGiver,
    Innkeeper,
}

/// Which coordinate space a spawn lives in. Dungeon spawns are tile coordinates inside the
/// generated map; overworld spawns are sample coordinates on the terrain field. The two
/// spaces are independent — consumers must never compare across them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnDomain {
    Dungeon,
    Overworld,
}

/// One placed actor.
#[derive(Clone, Debug, PartialEq)]
pub struct Spawn {
    pub domain: SpawnDomain,
    pub position: (u32, u32),
    pub kind: SpawnKind,
    pub level: u32,
    /// Which encounter group this actor belongs to (packs share an id).
    pub encounter: u32,
}

/// Population parameters.
#[derive(Clone, Debug, PartialEq)]
pub struct SpawnSpec {
    /// Base enemy level at the entrance; deeper is higher.
    pub base_level: u32,
    /// Enemy packs per normal room (max).
    pub packs_per_room: u32,
    /// Terrain scatter density: attempts per 100 samples.
    pub terrain_density_per_100: u32,
}

impl Default for SpawnSpec {
    fn default() -> Self {
        Self { base_level: 5, packs_per_room: 2, terrain_density_per_100: 1 }
    }
}

/// Weighted enemy pack archetypes: flavor families a consumer maps to creature templates.
const PACKS: [(&str, u32); 6] = [
    ("patrol", 30),
    ("ambush", 22),
    ("cultists", 18),
    ("beasts", 16),
    ("undead", 10),
    ("elemental", 4),
];

/// Populate the dungeon (rooms + graph distance) and the terrain field (biome scatter).
/// Both outputs share one encounter-id space so consumers can treat them uniformly.
pub fn populate(rng: &mut ProcRng, spec: &SpawnSpec, dungeon: &Dungeon, terrain: &TerrainField) -> Vec<Spawn> {
    let mut spawns = Vec::new();
    let mut next_encounter = 0u32;

    // --- dungeon rooms, by archetype and depth ---
    // Graph distances from the entrance (same BFS as generation).
    let distances = bfs(dungeon.rooms.len(), dungeon.entrance, &dungeon.rooms);
    for room in &dungeon.rooms {
        let tiles = dungeon.room_tiles(room);
        if tiles.is_empty() {
            continue;
        }
        let depth = distances[room.id].min(u32::MAX - 4);
        let level = spec.base_level + depth;
        match room.archetype {
            RoomArchetype::Entrance => {
                // Safe by contract: one vendor, one quest giver, never an enemy.
                let (vx, vy) = *rng.choose(&tiles);
                spawns.push(Spawn { domain: SpawnDomain::Dungeon, position: (vx, vy), kind: SpawnKind::Npc { role: NpcRole::Vendor }, level: 1.max(spec.base_level), encounter: next_encounter });
                next_encounter += 1;
                let (qx, qy) = *rng.choose(&tiles);
                spawns.push(Spawn { domain: SpawnDomain::Dungeon, position: (qx, qy), kind: SpawnKind::Npc { role: NpcRole::QuestGiver }, level: 1.max(spec.base_level), encounter: next_encounter });
                next_encounter += 1;
            }
            RoomArchetype::Boss => {
                // Exactly one boss at the room's center, two adds at random tiles.
                let (bx, by) = room.center();
                spawns.push(Spawn { domain: SpawnDomain::Dungeon, position: (bx, by), kind: SpawnKind::Enemy { archetype: "boss", boss: true }, level: level + 3, encounter: next_encounter });
                next_encounter += 1;
                let encounter = next_encounter;
                for _ in 0..2 {
                    let (x, y) = *rng.choose(&tiles);
                    spawns.push(Spawn { domain: SpawnDomain::Dungeon, position: (x, y), kind: SpawnKind::Enemy { archetype: "guard", boss: false }, level: level + 1, encounter });
                }
                next_encounter += 1;
            }
            RoomArchetype::Treasure => {
                // Guarded: one pack, one level hotter than the norm.
                let (x, y) = *rng.choose(&tiles);
                let pack = *rng.weighted(&PACKS);
                spawns.push(Spawn { domain: SpawnDomain::Dungeon, position: (x, y), kind: SpawnKind::Enemy { archetype: pack, boss: false }, level: level + 1, encounter: next_encounter });
                next_encounter += 1;
            }
            RoomArchetype::Normal => {
                let pack_count = rng.int(1, spec.packs_per_room as i64) as u32;
                for _ in 0..pack_count {
                    let pack = *rng.weighted(&PACKS);
                    let encounter = next_encounter;
                    let size = rng.int(2, 4) as u32;
                    for _ in 0..size {
                        let (x, y) = *rng.choose(&tiles);
                        spawns.push(Spawn { domain: SpawnDomain::Dungeon, position: (x, y), kind: SpawnKind::Enemy { archetype: pack, boss: false }, level, encounter });
                    }
                    next_encounter += 1;
                }
            }
        }
    }

    // --- terrain scatter: biome-appropriate families, level by distance from the field's
    //     center (the assumed player start), skipping ocean tiles ---
    let center = (terrain.width as i64 / 2, terrain.height as i64 / 2);
    let attempts = (terrain.width * terrain.height / 100 * spec.terrain_density_per_100).max(1);
    for _ in 0..attempts {
        let x = rng.int(0, terrain.width as i64 - 1) as u32;
        let y = rng.int(0, terrain.height as i64 - 1) as u32;
        let biome = terrain.biome_at(x, y);
        if biome == Biome::Ocean {
            continue;
        }
        let family = match biome {
            Biome::Desert => "beasts",
            Biome::Forest => "beasts",
            Biome::Plains => "patrol",
            Biome::Hills => "undead",
            Biome::Mountain | Biome::Snow => "elemental",
            _ => "patrol",
        };
        let distance = ((x as i64 - center.0).abs() + (y as i64 - center.1).abs()) / 16;
        let level = (spec.base_level + distance as u32).clamp(1, 80);
        spawns.push(Spawn { domain: SpawnDomain::Overworld, position: (x, y), kind: SpawnKind::Enemy { archetype: family, boss: false }, level, encounter: next_encounter });
        next_encounter += 1;
    }

    spawns
}

fn bfs(count: usize, from: usize, rooms: &[crate::dungeon::Room]) -> Vec<u32> {
    let mut dist = vec![u32::MAX; count];
    dist[from] = 0;
    let mut frontier = vec![from];
    while let Some(current) = frontier.pop() {
        for &next in &rooms[current].connections {
            if dist[next] == u32::MAX {
                dist[next] = dist[current] + 1;
                frontier.push(next);
            }
        }
    }
    dist
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dungeon::{generate as gen_dungeon, DungeonSpec};
    use crate::terrain::{generate as gen_terrain, TerrainSpec};

    fn world(seed: u64) -> (Dungeon, TerrainField, Vec<Spawn>) {
        let rng = ProcRng::new(seed);
        let dungeon = gen_dungeon(&mut rng.fork(crate::dungeon::DOMAIN), &DungeonSpec::default());
        let terrain = gen_terrain(&mut rng.fork(crate::terrain::DOMAIN), &TerrainSpec::default());
        let spawns = populate(&mut rng.fork(DOMAIN), &SpawnSpec::default(), &dungeon, &terrain);
        (dungeon, terrain, spawns)
    }

    #[test]
    fn entrance_safe_boss_unique_levels_progress() {
        for seed in 0..10 {
            let (dungeon, _, spawns) = world(seed);
            let entrance_tiles: Vec<(u32, u32)> = dungeon.room_tiles(&dungeon.rooms[dungeon.entrance]);
            // No dungeon-domain enemy stands in the entrance room (overworld spawns live in
            // a different coordinate space and are excluded by domain).
            assert!(
                spawns.iter().all(|s| s.domain != SpawnDomain::Dungeon
                    || !matches!(s.kind, SpawnKind::Enemy { .. })
                    || !entrance_tiles.contains(&s.position)),
                "seed {seed}: enemy in the entrance room"
            );
            // Exactly one boss, and it is in the boss room.
            let bosses: Vec<&Spawn> = spawns
                .iter()
                .filter(|s| matches!(s.kind, SpawnKind::Enemy { boss: true, .. }))
                .collect();
            assert_eq!(bosses.len(), 1, "seed {seed}: boss count {}", bosses.len());
            assert!(dungeon.rooms[dungeon.boss_room].contains(bosses[0].position.0, bosses[0].position.1));
            // Level progression: boss level exceeds the base.
            assert!(bosses[0].level > 5);
        }
    }

    #[test]
    fn same_seed_same_population() {
        assert_eq!(world(42).2, world(42).2);
    }

    #[test]
    fn npcs_present_and_enemies_leveled() {
        let (_, _, spawns) = world(42);
        assert!(spawns.iter().any(|s| matches!(s.kind, SpawnKind::Npc { .. })));
        assert!(spawns.iter().filter(|s| matches!(s.kind, SpawnKind::Enemy { .. })).all(|s| s.level >= 1 && s.level <= 80));
    }
}
