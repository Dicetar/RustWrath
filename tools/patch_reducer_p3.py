"""Patch: observation-relevant change detection in the reducer (for state_revision)."""
upath = "crates/benilla-protocol-wrath/src/updates.rs"
u = open(upath, encoding="utf-8").read()

old = """/// One movement snapshot a reducer keeps: identity-bearing fields only."""
marker = "/// The movement snapshot a reducer keeps: identity-bearing fields only."
assert marker in u
# append an equivalence helper after the MovementSnapshot struct definition.
anchor = """    /// The nine living-block speeds in wire order; `None` for non-living objects.
    pub speeds: Option<[f32; 9]>,
}"""
addition = """    /// The nine living-block speeds in wire order; `None` for non-living objects.
    pub speeds: Option<[f32; 9]>,
}

impl MovementSnapshot {
    /// Whether two snapshots differ in anything the observation schema can see. The wire's
    /// per-packet timestamp (`time`) is a clock, not state, and is deliberately excluded: a
    /// periodic heartbeat with identical position, flags and speeds is not a source state
    /// change and must not advance `state_revision`.
    pub fn observation_equivalent(&self, other: &Self) -> bool {
        self.move_flags == other.move_flags
            && self.extra_flags == other.extra_flags
            && self.position == other.position
            && self.speeds == other.speeds
    }
}"""
assert anchor in u
u = u.replace(anchor, addition, 1)
open(upath, "w", encoding="utf-8", newline="\n").write(u)
print("updates.rs: equivalence helper added")

rpath = "crates/benilla-protocol-wrath/src/reducer.rs"
r = open(rpath, encoding="utf-8").read()

# apply_values returns whether any delivered value actually differed.
old = """    fn apply_values(&mut self, guid: u64, r: &mut Reader) -> Result<()> {
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
new = """    /// Apply one values block; returns whether any delivered value actually differed from
    /// the stored field. Identical rewrites are not source state changes.
    fn apply_values(&mut self, guid: u64, r: &mut Reader) -> Result<bool> {
        let updates = values_block(r)?;
        let entry = self.entry(guid, 0);
        let mut changed = false;
        for (index, value) in updates {
            let i = index as usize;
            if entry.fields.get(i) != Some(&value) {
                changed = true;
            }
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
        Ok(changed)
    }"""
assert old in r, "apply_values anchor missing"
r = r.replace(old, new, 1)

# reduce signature: (events, changed). Public reduce keeps Vec for compat; add reduce_changed.
old = """    /// Reduce one world packet into the state, returning what changed.
    pub fn reduce(&mut self, opcode: u16, payload: &[u8]) -> Result<Vec<ChangeEvent>> {
        match opcode {"""
new = """    /// Reduce one world packet into the state, returning what changed.
    pub fn reduce(&mut self, opcode: u16, payload: &[u8]) -> Result<Vec<ChangeEvent>> {
        Ok(self.reduce_changed(opcode, payload)?.0)
    }

    /// Reduce one world packet, returning the change events and whether
    /// observation-relevant state changed (a movement block that only advanced its wire
    /// timestamp, or an identical values rewrite, is not a state change).
    pub fn reduce_changed(
        &mut self,
        opcode: u16,
        payload: &[u8],
    ) -> Result<(Vec<ChangeEvent>, bool)> {
        match opcode {"""
assert old in r, "reduce signature anchor missing"
r = r.replace(old, new, 1)

old = """            opcode::SMSG_COMPRESSED_UPDATE_OBJECT => {
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
            }
            _ => Ok(Vec::new()),
        }
    }"""
new = """            opcode::SMSG_COMPRESSED_UPDATE_OBJECT => {
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
            }
            _ => Ok((Vec::new(), false)),
        }
    }"""
assert old in r, "compressed arm anchor missing"
r = r.replace(old, new, 1)

old = """    fn reduce_update_object(&mut self, payload: &[u8]) -> Result<Vec<ChangeEvent>> {
        let mut r = Reader::new(payload);
        let mut events = Vec::new();
        let blocks = r.u32().context("block count")?;
        for _ in 0..blocks {
            let block_type = r.u8().context("block type")?;
            match block_type {
                0 => {
                    // VALUES: packed guid, values.
                    let guid = r.packed_guid()?;
                    self.apply_values(guid, &mut r)?;
                    events.push(ChangeEvent::Updated { guid });
                }
                1 => {
                    // MOVEMENT: packed guid, one movement block.
                    let guid = r.packed_guid()?;
                    let snapshot = movement_block(&mut r)?;
                    self.entry(guid, 0).movement = Some(snapshot);
                    events.push(ChangeEvent::Moved { guid });
                }
                0xAA => {
                    // SMSG_DESTROY_OBJECT (`Object.cpp:278-286`): full u64 GUID (not packed)
                    // plus an on-death flag the reducer does not need.
                    let guid = r.u64().context("destroy guid")?;
                    let _on_death = r.u8().context("destroy on-death flag")?;
                    self.remove(guid, &mut events);
                }
                2 | 3 => {"""
new = """    fn reduce_update_object(&mut self, payload: &[u8]) -> Result<(Vec<ChangeEvent>, bool)> {
        let mut r = Reader::new(payload);
        let mut events = Vec::new();
        let mut changed = false;
        let blocks = r.u32().context("block count")?;
        for _ in 0..blocks {
            let block_type = r.u8().context("block type")?;
            match block_type {
                0 => {
                    // VALUES: packed guid, values.
                    let guid = r.packed_guid()?;
                    changed |= self.apply_values(guid, &mut r)?;
                    events.push(ChangeEvent::Updated { guid });
                }
                1 => {
                    // MOVEMENT: packed guid, one movement block.
                    let guid = r.packed_guid()?;
                    let snapshot = movement_block(&mut r)?;
                    let relevant = match self.objects.get(&guid).and_then(|e| e.movement.as_ref()) {
                        Some(prev) => !prev.observation_equivalent(&snapshot),
                        None => true,
                    };
                    changed |= relevant;
                    self.entry(guid, 0).movement = Some(snapshot);
                    events.push(ChangeEvent::Moved { guid });
                }
                0xAA => {
                    // SMSG_DESTROY_OBJECT (`Object.cpp:278-286`): full u64 GUID (not packed)
                    // plus an on-death flag the reducer does not need.
                    let guid = r.u64().context("destroy guid")?;
                    let _on_death = r.u8().context("destroy on-death flag")?;
                    self.remove(guid, &mut events);
                    changed = true;
                }
                2 | 3 => {"""
assert old in r, "reduce_update_object head anchor missing"
r = r.replace(old, new, 1)

old = """                4 => {
                    // OUT_OF_RANGE_OBJECTS: u32 count, packed guids (`UpdateData.cpp:58-66`).
                    let count = r.u32().context("out-of-range count")?;
                    for _ in 0..count {
                        let guid = r.packed_guid()?;
                        self.remove(guid, &mut events);
                    }
                }
                other => bail!("unknown update block type {other}"),
            }
        }
        if !r.done() {
            bail!("{} trailing bytes after the last update block", r.p.len() - r.at);
        }
        Ok(events)
    }"""
new = """                4 => {
                    // OUT_OF_RANGE_OBJECTS: u32 count, packed guids (`UpdateData.cpp:58-66`).
                    let count = r.u32().context("out-of-range count")?;
                    for _ in 0..count {
                        let guid = r.packed_guid()?;
                        self.remove(guid, &mut events);
                    }
                    changed = true;
                }
                other => bail!("unknown update block type {other}"),
            }
        }
        if !r.done() {
            bail!("{} trailing bytes after the last update block", r.p.len() - r.at);
        }
        Ok((events, changed))
    }"""
assert old in r, "reduce_update_object tail anchor missing"
r = r.replace(old, new, 1)

open(rpath, "w", encoding="utf-8", newline="\n").write(r)
print("reducer.rs: change detection wired")
