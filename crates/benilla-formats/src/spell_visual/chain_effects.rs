//! `SpellChainEffects.dbc`: the beams of Chain Lightning, Drain Life, Mind Flay, Chain Heal. A kit
//! reaches it only through a CharProc slot of type 0 or 12, both of which the dispatcher
//! (`0x60d7c0`, table `0x60dc20`) routes to one case (`0x60da79`) and on to `CreateChainVisual`
//! (`0x6ecbd0`). Ids skip 14 and 16, so an id is a lookup, never an index.
//! `benilla-extract <Data> chaincensus` prints the table with every kit that draws a beam.

use std::collections::HashMap;
use std::io::Cursor;

use anyhow::{bail, Context, Result};
use benilla_bytes::ByteExt;
use benilla_dbc::{DbcParser, FieldType, Schema, SchemaField};

use crate::dbc::{f32_at, field_count, parse, str_at, u32_at};
use crate::Chain;

const SPELL_CHAIN_EFFECTS: &str = "DBFilesClient\\SpellChainEffects.dbc";
const SPELL_CHAIN_EFFECTS_FIELDS: usize = 8;

/// The client's clamp on a chain proc's beam count (`0x6ecbd0`); only kit 6397, Chain Burn, asks
/// for more than one.
pub const CHAIN_MAX_BEAMS: u32 = 3;

/// One row: the shape and animation of one beam. Fields 4-6 are not what their community names
/// (`TexCoordScale`, `SegDuration`, `SegDelay`) say.
#[derive(Debug, Clone, PartialEq)]
pub struct ChainEffect {
    /// Field 1: the target segment length; a hop gets `trunc(length / this + 2.0)` segments, so
    /// never fewer than two (`0x7af713`).
    pub avg_seg_len: f32,
    /// Field 2: the half-width; the ribbon spans twice this in yards.
    pub half_width: f32,
    /// Field 3: the jitter amplitude as a fraction of the beam's length, re-rolled every frame
    /// (`0x7b0950`) and blended 0.75 old to 0.25 new; 0.01 on most rows, 0.04 on Chain Lightning's.
    pub noise_scale: f32,
    /// Field 4: the texture's scroll period in seconds, `u = -(phase / this)` with `phase`
    /// advancing by `dt` modulo this (`0x7af9d7`). A negative period reverses the scroll: four
    /// rows, the drains among them, ship -0.5, flowing back toward the caster.
    pub scroll_period_s: f32,
    /// Field 5: how long one hop burns in ms (`0x6ec9eb`), and so the beam's expiry,
    /// `now + hops × this` (`0x6ecd30`); a channel beam ignores it and lives until swept.
    pub bolt_life_ms: u32,
    /// Field 6: the stagger between hops in ms, hop `i` lighting at `t0 + i × this` (`0x6ec9da`),
    /// so a 3-hop cast arcs outward; a channel beam bypasses it.
    pub bolt_stagger_ms: u32,
    /// Field 7: the texture path as stored (`Textures\SpellChainEffects\*.blp`).
    pub texture: String,
}

/// A kit's chain CharProc, its params read through [`super::char_proc_small_int`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainProc {
    /// `CharParamZero`, the [`ChainEffect`] id; never 0, as the client no-ops an id with no row
    /// (`0x6ecc0e`).
    pub effect_id: u32,
    /// `CharParamOne`, clamped to [`CHAIN_MAX_BEAMS`]; the client draws nothing for 0.
    pub beams: u32,
    /// `CharParamTwo` as a bool (`setne`): set on channel-stage kits, clear on cast-stage ones, and
    /// the only real difference between the two chain types.
    pub flag: bool,
    /// The stored type, 0 or 12; behaviour keys off [`Self::flag`], as the dispatcher never forks.
    pub ty: i32,
}

fn chain_effects_schema() -> Schema {
    let mut s = Schema::new("SpellChainEffects");
    let mut add = |name: &str, ty| s.add_field(SchemaField::new(name.to_string(), ty));
    add("ID", FieldType::UInt32);
    add("AvgSegLen", FieldType::Float32);
    add("HalfWidth", FieldType::Float32);
    add("NoiseScale", FieldType::Float32);
    add("ScrollPeriod", FieldType::Float32);
    add("BoltLife", FieldType::UInt32);
    add("BoltStagger", FieldType::UInt32);
    add("Texture", FieldType::String);
    debug_assert_eq!(s.fields.len(), SPELL_CHAIN_EFFECTS_FIELDS);
    s
}

/// Read `SpellChainEffects.dbc`; a textureless row stays, as the client's constructor never reads
/// the texture.
pub(super) fn load(chain: &mut Chain) -> Result<HashMap<u32, ChainEffect>> {
    let bytes = chain
        .read_file(SPELL_CHAIN_EFFECTS)
        .with_context(|| format!("reading {SPELL_CHAIN_EFFECTS}"))?;
    match field_count(&bytes) {
        Some(48) => return load_wrath(&bytes),
        Some(SPELL_CHAIN_EFFECTS_FIELDS) => {}
        Some(columns) => bail!(
            "unsupported SpellChainEffects.dbc layout: {columns} fields (expected 8 or 48)"
        ),
        None => bail!("parsing SpellChainEffects.dbc header: truncated field count"),
    }
    let set = parse(&bytes, chain_effects_schema(), "SpellChainEffects.dbc")?;
    let mut rows = HashMap::with_capacity(set.records().len());
    for r in set.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        rows.insert(
            id,
            ChainEffect {
                avg_seg_len: f32_at(r, 1).unwrap_or(0.0),
                half_width: f32_at(r, 2).unwrap_or(0.0),
                noise_scale: f32_at(r, 3).unwrap_or(0.0),
                scroll_period_s: f32_at(r, 4).unwrap_or(0.0),
                bolt_life_ms: u32_at(r, 5).unwrap_or(0),
                bolt_stagger_ms: u32_at(r, 6).unwrap_or(0),
                texture: str_at(&set, r, 7).unwrap_or_default(),
            },
        );
    }
    Ok(rows)
}

/// The 3.2-3.3.5 record has 48 fields but only 177 bytes: color/blend fields are bytes.
/// Its first eight fields retain the vanilla beam data. Read that verified prefix with
/// checked offsets; do not pretend a four-byte-per-field schema covers this packed row.
fn load_wrath(bytes: &[u8]) -> Result<HashMap<u32, ChainEffect>> {
    let parser = DbcParser::parse(&mut Cursor::new(bytes))
        .context("parsing SpellChainEffects.dbc header and body")?;
    let header = parser.header();
    if header.record_size != 177 {
        bail!(
            "unsupported SpellChainEffects.dbc record size: {} (expected 177 for 48 fields)",
            header.record_size
        );
    }
    let records_end = (header.record_count as usize)
        .checked_mul(177)
        .and_then(|size| size.checked_add(20))
        .context("SpellChainEffects.dbc record size overflow")?;
    let strings_end = records_end
        .checked_add(header.string_block_size as usize)
        .context("SpellChainEffects.dbc string block size overflow")?;
    let records = bytes
        .get(20..records_end)
        .context("SpellChainEffects.dbc truncated records")?;
    let strings = bytes
        .get(records_end..strings_end)
        .context("SpellChainEffects.dbc truncated string block")?;
    let mut rows = HashMap::with_capacity(header.record_count as usize);
    for row in records.chunks_exact(177) {
        let id = row.u32_at(0).context("SpellChainEffects.dbc missing id")?;
        let offset = row
            .u32_at(28)
            .context("SpellChainEffects.dbc missing texture offset")? as usize;
        let text = strings
            .get(offset..)
            .with_context(|| format!("SpellChainEffects.dbc {id}: invalid texture offset {offset}"))?;
        let end = text.iter().position(|byte| *byte == 0).unwrap_or(text.len());
        rows.insert(
            id,
            ChainEffect {
                avg_seg_len: row.f32_at(4).context("SpellChainEffects.dbc segment length")?,
                half_width: row.f32_at(8).context("SpellChainEffects.dbc width")?,
                noise_scale: row.f32_at(12).context("SpellChainEffects.dbc noise scale")?,
                scroll_period_s: row.f32_at(16).context("SpellChainEffects.dbc texture period")?,
                bolt_life_ms: row.u32_at(20).context("SpellChainEffects.dbc segment duration")?,
                bolt_stagger_ms: row.u32_at(24).context("SpellChainEffects.dbc segment delay")?,
                texture: String::from_utf8_lossy(&text[..end]).into_owned(),
            },
        );
    }
    Ok(rows)
}
