//! Room-and-corridor dungeons: BSP partition → carved rooms → guaranteed corridor
//! connectivity → archetype assignment by graph distance (entrance on the edge, boss at the
//! far end, treasure on a deep leaf). Output is both a typed room graph and a carved tile
//! map, so consumers can reason about structure or render cells directly.

use crate::rng::ProcRng;

/// The fork domain for the dungeon stream.
pub const DOMAIN: u64 = 0x4455_4e47_4f4e_0002; // "DUNGEON"

/// One tile of the carved map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tile {
    Solid,
    Floor,
    Corridor,
    Door,
}

impl Tile {
    pub fn discriminant_byte(self) -> u8 {
        match self {
            Tile::Solid => 0,
            Tile::Floor => 1,
            Tile::Corridor => 2,
            Tile::Door => 3,
        }
    }

    pub fn walkable(self) -> bool {
        !matches!(self, Tile::Solid)
    }
}

/// What a room is for. Assignment is by graph distance from the entrance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomArchetype {
    /// Safe entry: no enemies, the NPC home.
    Entrance,
    /// Standard encounter space.
    Normal,
    /// Guarded loot.
    Treasure,
    /// The far end: one boss room when the dungeon has more than one room.
    Boss,
}

/// A carved room in world-tile coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Room {
    pub id: usize,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub archetype: RoomArchetype,
    /// Adjacent rooms (corridor-connected), in connection insertion order.
    pub connections: Vec<usize>,
}

impl Room {
    pub fn center(&self) -> (u32, u32) {
        (self.x + self.w / 2, self.y + self.h / 2)
    }

    pub fn contains(&self, x: u32, y: u32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

/// A complete dungeon: the tile map plus the typed room graph.
#[derive(Clone, Debug, PartialEq)]
pub struct Dungeon {
    pub width: u32,
    pub height: u32,
    pub tiles: Vec<Tile>,
    pub rooms: Vec<Room>,
    pub entrance: usize,
    pub boss_room: usize,
}

/// Generation parameters. Defaults are a comfortable mid-size layout.
#[derive(Clone, Debug, PartialEq)]
pub struct DungeonSpec {
    /// Map bounds in tiles.
    pub width: u32,
    pub height: u32,
    /// Maximum room count; geometric limits may stop partitioning earlier.
    pub target_rooms: u32,
    /// Minimum room edge; BSP stops splitting below 2× this.
    pub min_room: u32,
    /// Extra loop corridors beyond the spanning chain (0 = pure tree).
    pub extra_loops: u32,
}

impl Default for DungeonSpec {
    fn default() -> Self {
        Self { width: 96, height: 64, target_rooms: 10, min_room: 8, extra_loops: 2 }
    }
}

struct Leaf {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    room: Option<usize>,
}

/// Generate one dungeon from `(rng, spec)`.
///
/// # Errors
/// [`DungeonSpec::validate`] rejects invalid dimensions and generation budgets.
pub fn generate(rng: &mut ProcRng, spec: &DungeonSpec) -> Result<Dungeon, ValidationError> {
    spec.validate()?;
    let width = spec.width;
    let height = spec.height;
    let mut tiles = vec![Tile::Solid; (width * height) as usize];

    // 1. BSP partition into leaves.
    let mut leaves: Vec<Leaf> = vec![Leaf { x: 1, y: 1, w: width - 2, h: height - 2, room: None }];
    let mut split_cursor = 0;
    while leaves.len() < spec.target_rooms as usize && split_cursor < leaves.len() {
        let leaf = &leaves[split_cursor];
        let can_split = leaf.w >= spec.min_room as u32 * 2 || leaf.h >= spec.min_room as u32 * 2;
        split_cursor += 1;
        if !can_split {
            continue;
        }
        let split_vertical = if leaf.w >= spec.min_room as u32 * 2 && leaf.h >= spec.min_room as u32 * 2 {
            rng.next_u64() & 1 == 0
        } else {
            leaf.w > leaf.h
        };
        let (a, b) = if split_vertical {
            let cut = rng.int(spec.min_room as i64, (leaf.w - spec.min_room as u32) as i64) as u32;
            (
                Leaf { x: leaf.x, y: leaf.y, w: cut, h: leaf.h, room: None },
                Leaf { x: leaf.x + cut, y: leaf.y, w: leaf.w - cut, h: leaf.h, room: None },
            )
        } else {
            let cut = rng.int(spec.min_room as i64, (leaf.h - spec.min_room as u32) as i64) as u32;
            (
                Leaf { x: leaf.x, y: leaf.y, w: leaf.w, h: cut, room: None },
                Leaf { x: leaf.x, y: leaf.y + cut, w: leaf.w, h: leaf.h - cut, room: None },
            )
        };
        // Replace the split leaf with its two children (room map by index).
        let last = leaves.len() - 1;
        leaves.swap(split_cursor - 1, last);
        leaves.pop(); // the split leaf, replaced by its two children below
        leaves.push(a);
        leaves.push(b);
        // Restart the scan: indices shifted.
        split_cursor = split_cursor.saturating_sub(1);
    }

    // 2. Carve a room inside each leaf.
    let mut rooms: Vec<Room> = Vec::new();
    for leaf in &mut leaves {
        let max_w = leaf.w.saturating_sub(2).max(spec.min_room);
        let max_h = leaf.h.saturating_sub(2).max(spec.min_room);
        let w = rng.int(spec.min_room as i64, max_w as i64).max(3) as u32;
        let h = rng.int(spec.min_room as i64, max_h as i64).max(3) as u32;
        let x = leaf.x + rng.int(0, (leaf.w - w) as i64) as u32;
        let y = leaf.y + rng.int(0, (leaf.h - h) as i64) as u32;
        for ty in y..y + h {
            for tx in x..x + w {
                tiles[(ty * width + tx) as usize] = Tile::Floor;
            }
        }
        leaf.room = Some(rooms.len());
        rooms.push(Room { id: rooms.len(), x, y, w, h, archetype: RoomArchetype::Normal, connections: Vec::new() });
    }

    // 3. Connect: chain leaves in reading order (guaranteed spanning), then extra loops.
    let connect = |a: usize, b: usize, tiles: &mut Vec<Tile>, rooms: &mut Vec<Room>, rng: &mut ProcRng| {
        if a == b || rooms[a].connections.contains(&b) {
            return;
        }
        let (ax, ay) = rooms[a].center();
        let (bx, by) = rooms[b].center();
        // L-shaped corridor; horizontal first or vertical first by coin.
        let horizontal_first = rng.next_u64() & 1 == 0;
        let carve = |tiles: &mut Vec<Tile>, x0: u32, x1: u32, y0: u32, y1: u32| {
            for x in x0.min(x1)..=x0.max(x1) {
                for y in y0.min(y1)..=y0.max(y1) {
                    let i = (y * width + x) as usize;
                    if tiles[i] == Tile::Solid {
                        tiles[i] = Tile::Corridor;
                    }
                }
            }
        };
        if horizontal_first {
            carve(tiles, ax, bx, ay, ay);
            carve(tiles, bx, bx, ay, by);
        } else {
            carve(tiles, ax, ax, ay, by);
            carve(tiles, ax, bx, by, by);
        }
        rooms[a].connections.push(b);
        rooms[b].connections.push(a);
    };
    for pair in 0..rooms.len().saturating_sub(1) {
        connect(pair, pair + 1, &mut tiles, &mut rooms, rng);
    }
    for _ in 0..spec.extra_loops {
        let a = rng.int(0, rooms.len() as i64 - 1) as usize;
        let b = rng.int(0, rooms.len() as i64 - 1) as usize;
        connect(a, b, &mut tiles, &mut rooms, rng);
    }

    // 4. Archetypes by graph distance from the edge-nearest entrance.
    let entrance = edge_nearest_room(&rooms, width, height);
    let distances = bfs_distances(rooms.len(), entrance, &rooms);
    // Single-room dungeons: there is no separate boss room, so the entrance keeps its safe
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
    }
    // Treasure: the deepest room that is not entrance/boss and has the fewest connections.
    let treasure = distances
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != entrance && *i != boss_room)
        .min_by_key(|(i, d)| (rooms[*i].connections.len(), i64::MAX - **d as i64))
        .map(|(i, _)| i);
    if let Some(t) = treasure {
        rooms[t].archetype = RoomArchetype::Treasure;
    }

    // A door is an exterior corridor cell sharing an edge with the room's floor.
    for room in &rooms {
        for x in room.x..room.x + room.w {
            for y in [room.y.saturating_sub(1), (room.y + room.h).min(height - 1)] {
                let i = (y * width + x) as usize;
                if tiles[i] == Tile::Corridor {
                    tiles[i] = Tile::Door;
                }
            }
        }
        for y in room.y..room.y + room.h {
            for x in [room.x.saturating_sub(1), (room.x + room.w).min(width - 1)] {
                let i = (y * width + x) as usize;
                if tiles[i] == Tile::Corridor {
                    tiles[i] = Tile::Door;
                }
            }
        }
    }

    Ok(Dungeon { width, height, tiles, rooms, entrance, boss_room })
}

fn edge_nearest_room(rooms: &[Room], width: u32, height: u32) -> usize {
    rooms
        .iter()
        .map(|r| {
            let (cx, cy) = r.center();
            let dx = cx.min(width - cx);
            let dy = cy.min(height - cy);
            (r.id, dx.min(dy))
        })
        .min_by_key(|(_, d)| *d)
        .map(|(i, _)| i)
        .unwrap_or(0)
}

fn bfs_distances(count: usize, from: usize, rooms: &[Room]) -> Vec<u32> {
    let mut dist = vec![u32::MAX; count];
    dist[from] = 0;
    // FIFO discovery gives shortest distances even when loop corridors are present.
    let mut frontier = std::collections::VecDeque::from([from]);
    while let Some(current) = frontier.pop_front() {
        for &next in &rooms[current].connections {
            if dist[next] == u32::MAX {
                dist[next] = dist[current] + 1;
                frontier.push_back(next);
            }
        }
    }
    dist
}

impl Dungeon {
    /// Walkable tiles inside one room.
    pub fn room_tiles(&self, room: &Room) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        for y in room.y..room.y + room.h {
            for x in room.x..room.x + room.w {
                if self.tiles[(y * self.width + x) as usize].walkable() {
                    out.push((x, y));
                }
            }
        }
        out
    }

    /// Reachability of every walkable tile from the entrance room's center.
    pub fn all_reachable(&self) -> bool {
        let start = self.rooms[self.entrance].center();
        let mut seen = vec![false; (self.width * self.height) as usize];
        let mut queue = vec![start];
        seen[(start.1 * self.width + start.0) as usize] = true;
        while let Some((x, y)) = queue.pop() {
            for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx < 0 || ny < 0 || nx >= self.width as i32 || ny >= self.height as i32 {
                    continue;
                }
                let i = (ny as u32 * self.width + nx as u32) as usize;
                if !seen[i] && self.tiles[i].walkable() {
                    seen[i] = true;
                    queue.push((nx as u32, ny as u32));
                }
            }
        }
        self.tiles
            .iter()
            .enumerate()
            .all(|(i, t)| !t.walkable() || seen[i])
    }
}

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

impl ValidationError {
    pub(crate) fn new(field: &'static str, reason: &str) -> Self {
        Self { field, reason: reason.to_owned() }
    }
}

impl DungeonSpec {
    /// Rejects parameter combinations that cannot produce a playable layout.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let fail = |field: &'static str, reason: &str| -> Result<(), ValidationError> {
            Err(ValidationError { field, reason: reason.to_owned() })
        };
        if !(1..=1024).contains(&self.target_rooms) {
            return fail("dungeon.target_rooms", "expected 1..=1024 rooms");
        }
        if self.min_room < 3 {
            return fail("dungeon.min_room", "rooms smaller than 3 tiles cannot hold a center");
        }
        let min_side = self.min_room.checked_mul(4)
            .ok_or_else(|| ValidationError::new("dungeon.min_room", "dimension overflow"))?;
        if self.width > 2048 || self.height > 2048
            || u64::from(self.width) * u64::from(self.height) > 1_048_576
        {
            return fail("dungeon.dimensions", "maximum side 2048 and maximum 1048576 tiles");
        }
        if self.extra_loops > 4096 {
            return fail("dungeon.extra_loops", "maximum 4096 extra corridor attempts");
        }
        if self.width < min_side {
            return fail("dungeon.width", "map width must fit four minimum room edges");
        }
        if self.height < min_side {
            return fail("dungeon.height", "map height must fit four minimum room edges");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_spec_same_dungeon() {
        let mut a = ProcRng::new(11);
        let mut b = ProcRng::new(11);
        assert_eq!(
            generate(&mut a, &DungeonSpec::default()).expect("valid dungeon"),
            generate(&mut b, &DungeonSpec::default()).expect("valid dungeon"),
        );
    }

    #[test]
    fn rooms_connected_and_reachable() {
        for seed in 0..25 {
            let dungeon = generate(&mut ProcRng::new(seed), &DungeonSpec::default()).expect("valid dungeon");
            assert!(dungeon.rooms.len() >= 5, "seed {seed}: room count {}", dungeon.rooms.len());
            assert!(dungeon.all_reachable(), "seed {seed}: unreachable walkable tiles");
            assert_eq!(dungeon.rooms[dungeon.entrance].archetype, RoomArchetype::Entrance);
            assert_eq!(dungeon.rooms[dungeon.boss_room].archetype, RoomArchetype::Boss);
            assert_ne!(dungeon.entrance, dungeon.boss_room);
        }
    }

    #[test]
    fn exactly_one_boss_and_one_entrance() {
        let dungeon = generate(&mut ProcRng::new(77), &DungeonSpec { target_rooms: 8, ..Default::default() }).expect("valid dungeon");
        assert_eq!(dungeon.rooms.iter().filter(|r| r.archetype == RoomArchetype::Boss).count(), 1);
        assert_eq!(dungeon.rooms.iter().filter(|r| r.archetype == RoomArchetype::Entrance).count(), 1);
    }
}
