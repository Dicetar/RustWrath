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

impl UiScript {
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
