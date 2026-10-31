"""Phase fixes: boundary doors, single-room, zero-density, validation, manifest coverage, v2 bump."""

# ---------- dungeon.rs ----------
p = "crates/benilla-procgen/src/dungeon.rs"
s = open(p, encoding="utf-8").read()

# 1. Boundary-based doors: replace the center-scan helper with a perimeter scan, and the
#    per-connection call with a per-room pass (doors are a room property, not a pair property).
old = """    // 5. Doors where a corridor meets a room edge.
    for room in &rooms {
        for &other in &room.connections {
            let (cx, cy) = corridor_door_tile(&rooms[room.id], &rooms[other], &tiles, width);
            if let Some((x, y)) = cx.zip(cy) {
                let i = (y * width + x) as usize;
                if tiles[i] == Tile::Corridor {
                    tiles[i] = Tile::Door;
                }
            }
        }
    }"""
new = """    // 5. Boundary-based doors: any corridor tile on a room's perimeter ring becomes a door.
    //    Scanning the ring (not the center) places doors where corridors actually touch the
    //    room, so multi-connection rooms get one door per real opening.
    for room in &rooms {
        for x in room.x.saturating_sub(1)..=(room.x + room.w).min(width - 1) {
            for y in [room.y.saturating_sub(1), (room.y + room.h).min(height - 1)] {
                let i = (y * width + x) as usize;
                if tiles[i] == Tile::Corridor {
                    tiles[i] = Tile::Door;
                }
            }
        }
        for y in room.y.saturating_sub(1)..=(room.y + room.h).min(height - 1) {
            for x in [room.x.saturating_sub(1), (room.x + room.w).min(width - 1)] {
                let i = (y * width + x) as usize;
                if tiles[i] == Tile::Corridor {
                    tiles[i] = Tile::Door;
                }
            }
        }
    }"""
assert old in s, "door loop anchor missing"
s = s.replace(old, new, 1)

# 2. Remove the obsolete center-scan helper entirely.
start = s.index("fn corridor_door_tile(")
end = s.index("impl Dungeon {")
s = s[:start] + s[end:]

# 3. Single-room handling: entrance wins the archetype; boss_room coincides but spawns no
#    boss when there is no separate room (the population layer reads the archetype).
old = """    let boss_room = distances
        .iter()
        .enumerate()
        .max_by_key(|(i, d)| (*d, rooms.len() - *i))
        .map(|(i, _)| i)
        .unwrap_or(entrance);
    rooms[entrance].archetype = RoomArchetype::Entrance;
    rooms[boss_room].archetype = RoomArchetype::Boss;"""
new = """    // Single-room dungeons: there is no separate boss room, so the entrance keeps its safe
    // archetype and `boss_room` coincides with it; the population layer keys boss placement
    // off the archetype pair, not the index equality.
    let boss_room = if rooms.len() <= 1 {
        entrance
    } else {
        distances
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != entrance)
            .max_by_key(|(i, d)| (*d, rooms.len() - *i))
            .map(|(i, _)| i)
            .unwrap_or(entrance)
    };
    rooms[entrance].archetype = RoomArchetype::Entrance;
    if boss_room != entrance {
        rooms[boss_room].archetype = RoomArchetype::Boss;
    }"""
assert old in s, "archetype anchor missing"
s = s.replace(old, new, 1)

# 4. Validation hook in generate().
old = """/// Generate one dungeon from `(rng, spec)`.
pub fn generate(rng: &mut ProcRng, spec: &DungeonSpec) -> Dungeon {
    let width = spec.width.max(spec.min_room * 4);"""
new = """/// Generate one dungeon from `(rng, spec)`.
///
/// # Errors
/// [`DungeonSpec::validate`] rejects zero/negative room targets, rooms below the minimum
/// carve size, and maps too small for the minimum room.
pub fn generate(rng: &mut ProcRng, spec: &DungeonSpec) -> Result<Dungeon, ValidationError> {
    spec.validate()?;
    let spec = spec;
    let width = spec.width.max(spec.min_room * 4);"""
assert old in s, "generate anchor missing"
s = s.replace(old, new, 1)

# the two `Dungeon { ... }` returns become Ok(...)
old = """    Dungeon { width, height, tiles, rooms, entrance, boss_room }
}"""
new = """    Ok(Dungeon { width, height, tiles, rooms, entrance, boss_room })
}"""
assert old in s, "return anchor missing"
s = s.replace(old, new, 1)

# 5. Validation type + impl at the end of dungeon.rs
s = s.rstrip() + """

/// A generation-parameter rejection. Carries the field and the reason.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationError {
    pub field: &'static str,
    pub reason: String,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.field, self.reason)
    }
}

impl std::error::Error for ValidationError {}

impl DungeonSpec {
    /// Rejects parameter combinations that cannot produce a playable layout.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let fail = |field, reason| Err(ValidationError { field, reason: reason.to_owned() });
        if self.target_rooms == 0 {
            return fail("target_rooms", "at least one room is required");
        }
        if self.min_room < 3 {
            return fail("min_room", "rooms smaller than 3 tiles cannot hold a center");
        }
        let min_side = self.min_room * 4;
        if self.width < min_side {
            return fail("width", "map width must fit four minimum rooms");
        }
        if self.height < min_side {
            return fail("height", "map height must fit four minimum rooms");
        }
        Ok(())
    }
}
"""
open(p, "w", encoding="utf-8", newline="\n").write(s)
print("dungeon.rs: doors/single-room/validation done")
