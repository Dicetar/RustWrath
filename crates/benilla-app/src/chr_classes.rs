//! `ChrClasses.dbc`, loaded once for its unrelated readers: the pet book (field 4, pet name token),
//! `UnitHasRelicSlot` (field 16) and `crate::spell::mods` (field 15, spell family). Absent when the
//! load fails; each reader then takes the reference's degraded answer: `"PET"`, no relic slot,
//! family 0.

use std::collections::HashMap;

use bevy::prelude::*;

use benilla_assets::{AssetSet, LockRecover, WorldAssets};
use benilla_formats::{ChrClasses, ChrRaces};
use benilla_ui::script::{LocalizedClassName, UiScript, UiTarget};

/// The parsed table; [`benilla_formats::ChrClasses`] documents the columns.
#[derive(Resource)]
pub(crate) struct ChrClassTable(pub(crate) ChrClasses);

/// `ChrRaces.dbc`'s names and tokens, at the install's locale column; `UnitRace` and the race words
/// the chat and social lists print come from it. Absent when the load fails, and no race has a name.
#[derive(Resource)]
pub(crate) struct ChrRaceTable(pub(crate) ChrRaces);

/// Each race's PvP side, the team digit `0x5efe00` reads off the chain: `ChrRaces.dbc` field 2 to
/// that `FactionTemplate.dbc` row's group mask, `& 4` Horde (0), else `& 2` Alliance (1), else no
/// side (-1). Loaded with the race table ([`benilla_formats::load_race_pvp_teams`]), so a chain
/// that adds races gives them their own side; absent when the load fails, and no race has a side.
#[derive(Resource, Clone, Debug, Default)]
pub(crate) struct RacePvpTeams(pub(crate) HashMap<u8, i8>);

impl RacePvpTeams {
    /// The race's team digit, -1 for a race without a row, the engine's bounds failure.
    pub(crate) fn team(&self, race: u8) -> i8 {
        self.0.get(&race).copied().unwrap_or(-1)
    }

    /// The side's `FactionGroup` token, `"Horde"` or `"Alliance"`, `None` for no side.
    pub(crate) fn faction_group(&self, race: u8) -> Option<&'static str> {
        match self.team(race) {
            0 => Some("Horde"),
            1 => Some("Alliance"),
            _ => None,
        }
    }

    /// The stock 1.12 chain's nine rows: group mask 3 (Alliance) for Human, Dwarf, Night Elf,
    /// Gnome and the unplayable Goblin, mask 5 (Horde) for Orc, Undead, Tauren and Troll.
    #[cfg(test)]
    pub(crate) fn stock() -> Self {
        Self(HashMap::from([
            (1, 1),
            (2, 0),
            (3, 1),
            (4, 1),
            (5, 0),
            (6, 0),
            (7, 1),
            (8, 0),
            (9, 1),
        ]))
    }
}

pub(crate) struct ChrClassesPlugin;

impl Plugin for ChrClassesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (load_chr_classes, load_chr_races).after(AssetSet::Open),
        );
    }
}

fn load_chr_classes(mut commands: Commands, assets: Option<Res<WorldAssets>>) {
    let Some(assets) = assets else { return };
    let loaded = {
        let mut chain = assets.chain.lock_recover();
        benilla_formats::load_chr_classes(&mut chain)
    };
    match loaded {
        Ok(table) => commands.insert_resource(ChrClassTable(table)),
        Err(e) => warn!(
            "chr_classes: ChrClasses.dbc failed to load — every pet book tab reads the client's \
             own \"PET\" fallback, so a warlock's says Pet rather than Demon, no class reads \
             as having a relic slot, and no talent spell modifier can apply (the gate has no \
             class family to match): {e:#}"
        ),
    }
}

/// The 12340 class-label rows for the UI's nine-locale table, projected from the one parsed
/// catalog. Every row must carry a native id that fits the binding's `u8`, a non-empty Filename,
/// and male/female labels at the locale: a short row is an explicit refusal, never a fabricated
/// table, and the binding remains unavailable until the catalog is complete.
pub(crate) fn localized_class_names(
    classes: &ChrClasses,
    locale_index: usize,
) -> Result<Vec<LocalizedClassName>, String> {
    let rows = classes.localized_labels(locale_index).ok_or_else(|| {
        format!(
            "ChrClasses.dbc is not the 12340 layout, or locale {locale_index} is outside the \
             client's nine-locale table"
        )
    })?;
    if rows.is_empty() {
        return Err("ChrClasses.dbc has no class rows; refusing an empty UI catalog".into());
    }
    let mut ids = std::collections::HashSet::new();
    let mut files = std::collections::HashSet::new();
    let mut projected = Vec::with_capacity(rows.len());
    for row in rows {
        let id = u8::try_from(row.id)
            .map_err(|_| format!("class id {} does not fit the UI binding's u8", row.id))?;
        if !ids.insert(id) {
            return Err(format!("ChrClasses.dbc has duplicate class id {id}"));
        }
        let file = row
            .file
            .filter(|file| !file.is_empty() && !file.contains('\0'))
            .ok_or_else(|| format!("ChrClasses.dbc class {id} has no usable Filename"))?;
        if !files.insert(file.clone()) {
            return Err(format!("ChrClasses.dbc has duplicate class Filename {file:?}"));
        }
        let male = row.male.ok_or_else(|| {
            format!("ChrClasses.dbc class {id} has no male label at locale {locale_index}")
        })?;
        let female = row.female.ok_or_else(|| {
            format!("ChrClasses.dbc class {id} has no female label at locale {locale_index}")
        })?;
        projected.push(LocalizedClassName {
            id,
            file,
            male,
            female,
        });
    }
    Ok(projected)
}

/// The class table for a load that can run before [`ChrClassesPlugin`]'s startup loader (the
/// glue VM is built at startup, before registration-order ties break the other way). The
/// instance is inserted so the pet book, relic and talent readers share it; an install with no
/// chain yet leaves the projection unavailable.
fn classes_for_ui(world: &mut World) -> Option<ChrClasses> {
    if let Some(table) = world.get_resource::<ChrClassTable>() {
        return Some(table.0.clone());
    }
    let loaded = {
        let assets = world.get_resource::<WorldAssets>()?;
        let mut chain = assets.chain.lock_recover();
        benilla_formats::load_chr_classes(&mut chain)
    };
    match loaded {
        Ok(table) => {
            world.insert_resource(ChrClassTable(table.clone()));
            Some(table)
        }
        Err(error) => {
            warn!(
                "chr_classes: ChrClasses.dbc failed to load ahead of the UI bootstrap: {error:#}"
            );
            None
        }
    }
}

fn localized_race_names(races: &ChrRaces, locale: usize) -> Result<Vec<(u8, String)>, String> {
    let rows = races.localized_labels(locale).ok_or_else(|| {
        format!("ChrRaces.dbc has no 12340 labels at locale {locale}")
    })?;
    rows.into_iter().filter(|row| matches!(row.id, 1..=8 | 10 | 11))
        .map(|row| {
            let id = u8::try_from(row.id).map_err(|_| format!("race {} exceeds the UI ID range", row.id))?;
            let name = row.male.ok_or_else(|| format!(
                "ChrRaces.dbc race {id} has no male/base label at locale {locale}"
            ))?;
            Ok((id, name))
        }).collect()
}

fn seat_localized_race_names(world: &mut World, script: &mut UiScript) {
    let table = if let Some(table) = world.get_resource::<ChrRaceTable>() {
        table.0.clone()
    } else {
        let loaded = world.get_resource::<WorldAssets>().map(|assets| {
            benilla_formats::load_chr_races(&mut assets.chain.lock_recover())
        });
        match loaded {
            Some(Ok(table)) => {
                world.insert_resource(ChrRaceTable(table.clone()));
                table
            }
            Some(Err(error)) => {
                warn!("ui_script: race catalog unavailable before metadata seating: {error:#}");
                return;
            }
            None => return,
        }
    };
    match localized_race_names(&table, script.locale()) {
        Ok(rows) => {
            if let Err(error) = script.set_localized_race_names(script.locale(), rows) {
                error!("ui_script: localized race projection refused: {error}");
            }
        }
        Err(error) => warn!("ui_script: localized race projection unavailable: {error}"),
    }
}

/// Seat the target VM's localized class/race projections, before GlueXML/FrameXML execute the stock
/// `Constants.lua`. A 1.12 VM is untouched; a 12340 VM without a complete catalog logs and
/// leaves `FillLocalizedClassList` unavailable rather than inventing labels.
pub(crate) fn seat_localized_class_names(world: &mut World, script: &mut UiScript) {
    if script.target() != UiTarget::Wrath335 {
        return;
    }
    let locale = script.locale();
    let Some(classes) = classes_for_ui(world) else {
        warn!("ui_script: Wrath is active but ChrClasses.dbc is unavailable; FillLocalizedClassList will refuse");
        return;
    };
    match localized_class_names(&classes, locale) {
        Ok(rows) => {
            if let Err(error) = script.set_localized_class_names(locale, rows) {
                error!("ui_script: the localized class catalog was refused: {error}");
            }
        }
        Err(error) => warn!("ui_script: FillLocalizedClassList catalog unavailable: {error}"),
    }
    seat_localized_race_names(world, script);
}

/// The same projection for a host that owns an archive chain but no Bevy store (direct-VM
/// fixtures, the worldview harness): the caller lets the refusal fail rather than warn.
pub(crate) fn seat_localized_class_names_from_chain(
    script: &mut UiScript,
    chain: &mut benilla_formats::Chain,
) -> Result<(), String> {
    if script.target() != UiTarget::Wrath335 {
        return Ok(());
    }
    let locale = script.locale();
    let classes = benilla_formats::load_chr_classes(chain).map_err(|error| {
        format!("ChrClasses.dbc failed to load ahead of the UI bootstrap: {error:#}")
    })?;
    let rows = localized_class_names(&classes, locale)?;
    script
        .set_localized_class_names(locale, rows)
        .map_err(|error| format!("the localized class catalog was refused: {error}"))?;
    let races = benilla_formats::load_chr_races(chain)
        .map_err(|error| format!("ChrRaces.dbc failed to load ahead of the UI bootstrap: {error:#}"))?;
    let rows = localized_race_names(&races, locale)?;
    script.set_localized_race_names(locale, rows)
        .map_err(|error| format!("the localized race catalog was refused: {error}"))
}

fn load_chr_races(mut commands: Commands, assets: Option<Res<WorldAssets>>) {
    let Some(assets) = assets else { return };
    let (loaded, teams) = {
        let mut chain = assets.chain.lock_recover();
        (
            benilla_formats::load_chr_races(&mut chain),
            benilla_formats::load_race_pvp_teams(&mut chain),
        )
    };
    match loaded {
        Ok(table) => commands.insert_resource(ChrRaceTable(table)),
        Err(e) => warn!(
            "chr_races: ChrRaces.dbc failed to load — no unit has a race name or token, so \
             `UnitRace` answers nil and the social lists print no race: {e:#}"
        ),
    }
    match teams {
        Ok(teams) => commands.insert_resource(RacePvpTeams(teams)),
        Err(e) => warn!(
            "chr_races: the races' PvP sides failed to load (ChrRaces.dbc to FactionTemplate.dbc) \
             — no race has a side, so every rank title and the roster seat's faction group read \
             none: {e:#}"
        ),
    }
}

