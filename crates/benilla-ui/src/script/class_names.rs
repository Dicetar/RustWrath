//! Wrath class labels supplied by the catalog owner before Constants.lua executes.
//! The target fills separate male/female tables. The independent Wowless implementation
//! (c09acebd9be7d52e41154dff5276f4d62f3e4970) confirms filename keys and table return;
//! it is a behavioral reference, not exact 12340 argument/error ABI evidence.

use std::{collections::HashSet, sync::Arc};

use mlua::{Lua, Table};

use super::{Model, UiScript, UiTarget};

/// A read projection of one native catalog row, not a second authoritative catalog.
/// Both labels must come from the selected locale's observed data. Empty observed labels
/// are preserved; the binding never substitutes English or the other gender's label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalizedClassName {
    pub id: u8,
    pub file: String,
    pub male: String,
    pub female: String,
}

#[derive(Clone, Debug)]
pub(crate) struct ClassNameSnapshot {
    pub locale: usize,
    pub rows: Arc<[LocalizedClassName]>,
}

// AzerothCore 12340 SharedDefines.h CLASSMASK_ALL_PLAYABLE includes Death Knight (6).
const PLAYABLE_CLASSES: [u8; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 11];
// 12340 playable races exclude Goblin (9); Blood Elf and Draenei are 10 and 11.
const PLAYABLE_RACES: [u8; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 10, 11];

struct RaceNameSnapshot {
    locale: usize,
    rows: std::collections::HashMap<u8, String>,
}

pub(super) fn playable_race_mask() -> i32 {
    PLAYABLE_RACES.iter().fold(0, |mask, &id| mask | (1 << (id - 1)))
}

pub(super) fn race_requirement_names(lua: &Lua, allowed: i32) -> mlua::Result<Vec<String>> {
    let locale = lua.app_data_ref::<Model>().expect("model").locale;
    let snapshot = lua.app_data_ref::<RaceNameSnapshot>()
        .filter(|snapshot| snapshot.locale == locale)
        .ok_or_else(|| mlua::Error::runtime("Item race requirements: localized catalog unavailable"))?;
    PLAYABLE_RACES.iter().filter(|&&id| allowed & (1 << (id - 1)) != 0)
        .map(|id| snapshot.rows.get(id).cloned().ok_or_else(|| mlua::Error::runtime(format!(
            "Item race requirements: localized race {id} unavailable"
        )))).collect()
}

pub(super) fn playable_class_mask() -> i32 {
    PLAYABLE_CLASSES.iter().fold(0, |mask, &id| mask | (1 << (id - 1)))
}

/// Item restrictions use the same selected-locale male labels as the seated class list.
/// Missing labels cannot silently remove a restricted class or substitute donor English.
pub(super) fn requirement_names(lua: &Lua, allowed: i32) -> mlua::Result<Vec<String>> {
    let model = lua.app_data_ref::<Model>().expect("model");
    let snapshot = model.localized_class_names.as_ref()
        .filter(|snapshot| snapshot.locale == model.locale)
        .ok_or_else(|| mlua::Error::runtime("Item class requirements: localized catalog unavailable"))?;
    PLAYABLE_CLASSES.iter().filter(|&&id| allowed & (1 << (id - 1)) != 0)
        .map(|&id| {
            snapshot.rows.iter().find(|row| row.id == id)
                .filter(|row| !row.male.is_empty())
                .map(|row| row.male.clone())
                .ok_or_else(|| mlua::Error::runtime(format!(
                    "Item class requirements: localized class {id} unavailable"
                )))
        }).collect()
}

impl UiScript {
    /// Seat selected-locale race labels from the existing ChrRaces owner, without Lua tables.
    /// Candidate validation precedes replacement, as for the class-label projection.
    pub fn set_localized_race_names(&mut self, locale: usize, rows: Vec<(u8, String)>) -> mlua::Result<()> {
        let model = self.model_ref();
        if model.target != UiTarget::Wrath335 || locale != model.locale {
            return Err(mlua::Error::runtime("Localized race projection requires the current Wrath locale"));
        }
        let mut names = std::collections::HashMap::new();
        for (id, name) in rows {
            if name.is_empty() || names.insert(id, name).is_some() {
                return Err(mlua::Error::runtime("Localized race projection has missing labels or duplicate IDs"));
            }
        }
        for id in PLAYABLE_RACES {
            if !names.contains_key(&id) {
                return Err(mlua::Error::runtime(format!("Localized race projection lacks playable race {id}")));
            }
        }
        drop(model);
        self.lua.set_app_data(RaceNameSnapshot { locale, rows: names });
        Ok(())
    }

    pub fn clear_localized_race_names(&mut self) {
        self.lua.remove_app_data::<RaceNameSnapshot>();
    }

    /// Publish an observed catalog before loading the target Constants.lua in either VM.
    /// Native IDs must be checked-converted by the caller, never truncated. Candidate
    /// validation precedes replacement; an invalid candidate preserves the previous view.
    /// Existing Lua tables are not refreshed: locale/profile changes require the owner to
    /// rebuild/reinitialize FrameXML and reseat this projection before its constants load.
    pub fn set_localized_class_names(
        &mut self,
        locale: usize,
        mut rows: Vec<LocalizedClassName>,
    ) -> mlua::Result<()> {
        let mut model = self.model_mut();
        if model.target != UiTarget::Wrath335 {
            return Err(mlua::Error::runtime("Localized class projection requires Wrath335"));
        }
        if locale != model.locale {
            return Err(mlua::Error::runtime("Localized class projection locale does not match the VM"));
        }
        let mut ids = HashSet::new();
        let mut files = HashSet::new();
        for row in &rows {
            if row.file.is_empty() || row.file.contains('\0') {
                return Err(mlua::Error::runtime("Localized class projection has an invalid file token"));
            }
            if !ids.insert(row.id) || !files.insert(row.file.as_str()) {
                return Err(mlua::Error::runtime("Localized class projection has duplicate IDs or file tokens"));
            }
        }
        rows.sort_by_key(|row| row.id);
        model.localized_class_names = Some(ClassNameSnapshot { locale, rows: rows.into() });
        Ok(())
    }

    /// Retire the read projection. This does not mutate tables already retained by Lua.
    pub fn clear_localized_class_names(&mut self) {
        self.model_mut().localized_class_names = None;
    }
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    lua.globals().set("FillLocalizedClassList", lua.create_function(
        |lua, (table, female): (Table, Option<bool>)| {
            let rows = {
                let model = lua.app_data_ref::<Model>().expect("model");
                let snapshot = model.localized_class_names.as_ref().ok_or_else(|| {
                    mlua::Error::runtime("FillLocalizedClassList: localized class catalog unavailable")
                })?;
                if snapshot.locale != model.locale {
                    return Err(mlua::Error::runtime("FillLocalizedClassList: catalog locale mismatch"));
                }
                Arc::clone(&snapshot.rows)
            };
            // Ordinary table writes can invoke __newindex: no Model borrow may survive here.
            // Preserve unrelated keys and return the supplied table, as the reference does.
            for row in rows.iter() {
                table.set(row.file.as_str(), if female.unwrap_or(false) {
                    row.female.as_str()
                } else {
                    row.male.as_str()
                })?;
            }
            Ok(table)
        },
    )?)
}
