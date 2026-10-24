"""Patch pass 1: bounded inflation + delivered-field semantics (Astra findings 1+2)."""
path = "crates/benilla-protocol-wrath/src/reducer.rs"
r = open(path, encoding="utf-8").read()
n0 = len(r)

# --- use line for Read trait ---
r = r.replace(
    "use std::collections::BTreeMap;",
    "use std::collections::BTreeMap;\nuse std::io::Read;",
    1,
)

# --- Fix 1: bounded inflation ---
old1 = """            opcode::SMSG_COMPRESSED_UPDATE_OBJECT => {
                // `WorldSocket.cpp:97-119`: u32 uncompressed size, then zlib of the plain
                // update-object payload — no inner opcode, no re-dispatch.
                let mut r = Reader::new(payload);
                let size = r.u32().context("compressed size")? as usize;
                let raw = flate2::read::ZlibDecoder::new(&payload[r.at..])
                    .bytes_read()
                    .context("inflating compressed update")?;
                if raw.len() != size {
                    bail!("compressed update size mismatch: header {size}, inflated {}", raw.len());
                }
                self.reduce_update_object(&raw)
            }"""
new1 = """            opcode::SMSG_COMPRESSED_UPDATE_OBJECT => {
                // `WorldSocket.cpp:97-119`: u32 uncompressed size, then zlib of the plain
                // update-object payload — no inner opcode, no re-dispatch.
                //
                // Inflation is bounded twice: the declared size must sit under a hard cap
                // before decompression starts, and the stream is read at most one byte past
                // the declared size, so actual output exceeding the declaration (or a bomb
                // under a plausible header) can never allocate freely.
                let mut r = Reader::new(payload);
                let size = r.u32().context("compressed size")? as usize;
                if size > MAX_INFLATED_UPDATE_BYTES {
                    bail!(
                        "declared inflated size {size} exceeds the hard cap {MAX_INFLATED_UPDATE_BYTES}"
                    );
                }
                let mut raw = Vec::with_capacity(size.min(65536));
                flate2::read::ZlibDecoder::new(&payload[r.at..])
                    .take(size as u64 + 1)
                    .read_to_end(&mut raw)
                    .context("inflating compressed update")?;
                if raw.len() != size {
                    bail!(
                        "compressed update size mismatch: declared {size}, inflated {}",
                        raw.len()
                    );
                }
                self.reduce_update_object(&raw)
            }"""
assert old1 in r, "fix1 anchor missing"
r = r.replace(old1, new1, 1)

# --- cap constant ---
old2 = "/// What one packet did to the world state, in arrival order."
new2 = """/// Hard cap on the inflated size of one compressed update packet: server-side, one buffered
/// send never exceeds 65536 bytes per write, so 1 MiB leaves orders of magnitude of headroom
/// while staying far below any decompression-bomb risk.
pub const MAX_INFLATED_UPDATE_BYTES: usize = 1024 * 1024;

/// What one packet did to the world state, in arrival order."""
assert old2 in r, "cap anchor missing"
r = r.replace(old2, new2, 1)

# --- Fix 2: delivered tracking ---
old3 = """#[derive(Clone, Debug, PartialEq)]
pub struct ObjectEntry {
    pub guid: u64,
    pub type_id: u8,
    pub fields: Vec<u32>,
    pub movement: Option<MovementSnapshot>,
    /// Increments when this GUID is destroyed and created again — identity check for the
    /// despawn/recreate dance visibility does.
    pub generation: u32,
}

impl ObjectEntry {
    /// A u32 field by index, if the mask ever delivered it.
    pub fn field(&self, index: u16) -> Option<u32> {
        self.fields.get(index as usize).copied()
    }"""
new3 = """#[derive(Clone, Debug, PartialEq)]
pub struct ObjectEntry {
    pub guid: u64,
    pub type_id: u8,
    pub fields: Vec<u32>,
    /// Which field indices any update mask delivered in this generation (one bit per u32
    /// field index); cleared on create. The basis for unknown-vs-zero semantics below.
    delivered: Vec<u64>,
    pub movement: Option<MovementSnapshot>,
    /// Increments when this GUID is destroyed and created again — identity check for the
    /// despawn/recreate dance visibility does.
    pub generation: u32,
}

impl ObjectEntry {
    /// A u32 field by index.
    ///
    /// Semantics (established, matching the stock client): the real client zero-initializes
    /// its field array at object creation and then applies delivered mask values, so an
    /// undelivered field reads as zero there too. `field()` returns that client-visible view;
    /// `None` only before any create/values block ever touched this GUID.
    ///
    /// For honest unknown-versus-zero distinction (export surfaces must not present a
    /// never-delivered zero as an observed zero), check [`ObjectEntry::field_delivered`].
    pub fn field(&self, index: u16) -> Option<u32> {
        self.fields.get(index as usize).copied()
    }

    /// Whether any update mask delivered this field index in the current generation. A field
    /// that is `Some(0)` with `field_delivered == false` is a client-view zero, not an
    /// observed zero.
    pub fn field_delivered(&self, index: u16) -> bool {
        let (word, bit) = (index as usize / 64, index as usize % 64);
        self.delivered.get(word).is_some_and(|w| w & (1 << bit) != 0)
    }"""
assert old3 in r, "fix2a anchor missing"
r = r.replace(old3, new3, 1)

old4 = """        self.objects.entry(guid).or_insert_with(|| ObjectEntry {
            guid,
            type_id,
            fields: Vec::new(),
            movement: None,
            generation: 0,
        })"""
new4 = """        self.objects.entry(guid).or_insert_with(|| ObjectEntry {
            guid,
            type_id,
            fields: Vec::new(),
            delivered: Vec::new(),
            movement: None,
            generation: 0,
        })"""
assert old4 in r, "fix2b anchor missing"
r = r.replace(old4, new4, 1)

old5 = """    fn apply_values(&mut self, guid: u64, r: &mut Reader) -> Result<()> {
        let updates = values_block(r)?;
        let entry = self.entry(guid, 0);
        for (index, value) in updates {
            let index = index as usize;
            if entry.fields.len() <= index {
                entry.fields.resize(index + 1, 0);
            }
            entry.fields[index] = value;
        }
        Ok(())
    }"""
new5 = """    fn apply_values(&mut self, guid: u64, r: &mut Reader) -> Result<()> {
        let updates = values_block(r)?;
        let entry = self.entry(guid, 0);
        for (index, value) in updates {
            let i = index as usize;
            if entry.fields.len() <= i {
                entry.fields.resize(i + 1, 0);
            }
            let (word, bit) = (i / 64, i % 64);
            if entry.delivered.len() <= word {
                entry.delivered.resize(word + 1, 0);
            }
            entry.delivered[word] |= 1 << bit;
            entry.fields[i] = value;
        }
        Ok(())
    }"""
assert old5 in r, "fix2c anchor missing"
r = r.replace(old5, new5, 1)

# --- create clears stale generation state ---
old6 = """                    let entry = self.entry(guid, type_id);
                    entry.generation = generation;
                    entry.movement = Some(snapshot);"""
new6 = """                    let entry = self.entry(guid, type_id);
                    entry.generation = generation;
                    // A recreate starts from a clean field slate: the stock client
                    // zero-initializes at creation, and stale values from a previous
                    // generation must not leak into the new one.
                    entry.fields.clear();
                    entry.delivered.clear();
                    entry.movement = Some(snapshot);"""
assert old6 in r, "create-clear anchor missing"
r = r.replace(old6, new6, 1)

open(path, "w", encoding="utf-8", newline="\n").write(r)
print(f"patched: {n0} -> {len(r)} bytes")
