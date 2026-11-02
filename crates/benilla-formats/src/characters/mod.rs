//! Character appearance render data: the geosets an appearance shows ([`CharacterGeosets`], the
//! client's geoset dispatch `0x477520`) and the body skin it wears: body-skin batches
//! (`M2TextureType::Other(1)`) have no texture of their own, and [`CharSections::composite_body`]
//! builds one on the 256² partition `0x475c50` and the section→cell map `0x4782e0`.
//!
//! DBC layouts follow the client's record readers: CharSections `0x575540`, the geoset tables
//! `0x5753b0`/`0x575a80`.
//! The compositor hides geosets 0..=1700 on 1.12 and 0..=2000 on 12340 (`GeosRenderPrep`);
//! higher IDs keep their model visibility through [`VisibleGeosets::contains`].

mod customization;
mod geosets;
mod sections;

pub use customization::{CharCreateCatalog, ChrRaceRow, DialRanges, NameGenRow, StartOutfitItem};
pub use geosets::{CharacterGeosets, EquipGeosets, GeosetVersion, VisibleGeosets};
pub use sections::{
    equip_blits, equip_column, equip_region_candidates, equip_tex_dir, equip_tile, forearm_dressed,
    BlitSource, CharSections, CompositePlan, EmblemLayer, EquipBlit, GuildEmblem,
};

// The loaders' own schemas, for the `benilla-extract dbc` CSV dump (`crate::schema_for`).
pub(crate) use geosets::{char_facial_hair_schema, char_hair_geosets_schema, helmet_vis_schema};
pub(crate) use sections::char_sections_schema;

use crate::Chain;

/// The 12340 `ChrRaces` layout's column count. 1.12 ships 29 fields, 12340 ships 69 (the selected
/// 3.3.5a data measures exactly 69), so the field count is the client-version probe every
/// character table that changed layout reads.
pub(crate) const WRATH_CHR_RACES_FIELDS: usize = 69;

/// Whether the chain's data is 12340, read off `ChrRaces.dbc`'s WDBC field count. An unreadable
/// table means the 1.12 layout, the default every caller here must keep.
pub(crate) fn wrath_client(chain: &mut Chain) -> bool {
    chain
        .read_file("DBFilesClient\\ChrRaces.dbc")
        .ok()
        .and_then(|bytes| crate::dbc::field_count(&bytes))
        .is_some_and(|fields| fields >= WRATH_CHR_RACES_FIELDS)
}
