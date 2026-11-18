//! The live Wrath → 1.12 translation script: Lua that runs after the field table for every
//! projected object, for translations a copy rule cannot express (a DBC lookup, a computed or
//! packed value).
//!
//! The shipped script (`wrath_translate.lua`, compiled in) runs first; an optional overlay,
//! `<benilla-config>/wrath-translate.lua`, runs after it in the same state, so it can redefine or
//! wrap the global `Translate(kind)` hook. The overlay is polled once a second; a reload builds a
//! fresh state and re-projects every streamed object, and a script that fails to load leaves the
//! previous one running. The client never writes the overlay.
//!
//! The hook sees one object at a time through these globals:
//! - `Raw(i)`: the delivered 3.3.5 descriptor word `i` of the object, or nil.
//! - `Get(i)`: the 1.12 word `i` this pass will write (the projection's, the table's or an
//!   earlier `Set`), or nil when this pass does not write it.
//! - `Set(i, v)`: write 1.12 word `i`; it wins over everything before it.
//! - `DbcRow(file, id)`: every column of the row with that id in `DBFilesClient\<file>.dbc`, as
//!   raw 32-bit numbers (a float column comes back as its bits), or nothing.
//! - `DbcString(file, offset)`: the string at that offset of the file's string block, or nil.
//! - `Log(text)`: an info line in the client log.
//!
//! A hook error drops that object's `Set`s for the pass and is reported once per load.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use benilla_assets::LockRecover;
use benilla_protocol_wrath::updates::type_id;
use benilla_ui::mlua::{self, Function, Lua, Variadic};
use bevy::prelude::*;

use crate::wrath::WrathEntity;
use crate::wrath_field_map::{FileChange, WatchedFile, WrathFieldMap};
use crate::wrath_inventory::WrathFieldWindow;

/// The script compiled into the client. Edit it in the repo for a permanent fix.
const SHIPPED: &str = include_str!("wrath_translate.lua");

/// The overlay's file name inside the config folder.
pub(crate) const OVERLAY_FILE: &str = "wrath-translate.lua";

/// The object the hook is running for.
#[derive(Default)]
struct Current {
    window: Option<WrathFieldWindow>,
    /// The pass's pairs before the hook.
    base: Vec<(u16, u32)>,
    /// The hook's `Set`s, in call order.
    sets: Vec<(u16, u32)>,
}

impl Current {
    fn get(&self, index: u16) -> Option<u32> {
        self.sets
            .iter()
            .rev()
            .chain(self.base.iter().rev())
            .find(|(i, _)| *i == index)
            .map(|(_, v)| *v)
    }
}

/// One raw WDBC file: the header's shape and each record's offset by its first column.
struct Dbc {
    bytes: Vec<u8>,
    fields: usize,
    strings_at: usize,
    rows: HashMap<u32, usize>,
}

fn le_word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

impl Dbc {
    fn parse(bytes: Vec<u8>) -> Result<Self, String> {
        if bytes.len() < 20 || &bytes[..4] != b"WDBC" {
            return Err("not a WDBC file".to_owned());
        }
        let records = le_word(&bytes, 4) as usize;
        let fields = le_word(&bytes, 8) as usize;
        let record_size = le_word(&bytes, 12) as usize;
        let string_size = le_word(&bytes, 16) as usize;
        let strings_at = records
            .checked_mul(record_size)
            .and_then(|size| size.checked_add(20));
        let fits = strings_at
            .and_then(|at| at.checked_add(string_size))
            .is_some_and(|end| end <= bytes.len());
        if fields == 0 || record_size < fields * 4 || !fits {
            return Err(format!(
                "bad header: {records} records of {fields} fields in {record_size} bytes, \
                 {string_size} string bytes, {} in the file",
                bytes.len()
            ));
        }
        let rows = (0..records)
            .map(|record| {
                let at = 20 + record * record_size;
                (le_word(&bytes, at), at)
            })
            .collect();
        Ok(Self {
            fields,
            strings_at: strings_at.unwrap_or_default(),
            rows,
            bytes,
        })
    }

    fn row(&self, id: u32) -> Option<Vec<u32>> {
        let at = *self.rows.get(&id)?;
        Some(
            (0..self.fields)
                .map(|field| le_word(&self.bytes, at + 4 * field))
                .collect(),
        )
    }

    fn string(&self, offset: u32) -> Option<String> {
        let start = self.strings_at.checked_add(offset as usize)?;
        let tail = self.bytes.get(start..)?;
        let end = tail.iter().position(|&b| b == 0)?;
        Some(String::from_utf8_lossy(&tail[..end]).into_owned())
    }
}

/// DBCs read through the open patch chain on first use. A file that fails is reported once and
/// stays absent until the chain changes.
#[derive(Default)]
struct Dbcs {
    chain: Option<Arc<Mutex<benilla_formats::Chain>>>,
    files: HashMap<String, Option<Dbc>>,
}

impl Dbcs {
    fn attach(&mut self, chain: &Arc<Mutex<benilla_formats::Chain>>) {
        if self
            .chain
            .as_ref()
            .is_some_and(|held| Arc::ptr_eq(held, chain))
        {
            return;
        }
        self.chain = Some(chain.clone());
        self.files.clear();
    }

    fn load(&self, name: &str) -> Result<Dbc, String> {
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err("not a DBC name".to_owned());
        }
        let Some(chain) = &self.chain else {
            return Err("the game data is not open".to_owned());
        };
        let bytes = chain
            .lock_recover()
            .read(&format!("DBFilesClient\\{name}.dbc"))
            .map_err(|error| format!("{error:#}"))?;
        Dbc::parse(bytes)
    }

    fn with<R>(&mut self, name: &str, read: impl FnOnce(&Dbc) -> Option<R>) -> Option<R> {
        if !self.files.contains_key(name) {
            let loaded = self.load(name);
            if let Err(error) = &loaded {
                warn!("wrath translate: {name}.dbc not available to the script: {error}");
            }
            self.files.insert(name.to_owned(), loaded.ok());
        }
        self.files.get(name)?.as_ref().and_then(read)
    }
}

/// The running script. Non-send: the Lua state lives on the main thread.
pub(crate) struct WrathTranslateScript {
    // Before `lua`, so the hook's reference drops while its state is alive.
    hook: Option<Function>,
    lua: Option<Lua>,
    current: Rc<RefCell<Current>>,
    dbcs: Rc<RefCell<Dbcs>>,
    file: WatchedFile,
    failed: Cell<bool>,
}

impl WrathTranslateScript {
    pub(crate) fn new() -> Self {
        let mut script = Self {
            hook: None,
            lua: None,
            current: Rc::default(),
            dbcs: Rc::default(),
            file: WatchedFile::new(OVERLAY_FILE),
            failed: Cell::new(false),
        };
        match script.build(None) {
            Ok((lua, hook)) => {
                script.hook = Some(hook);
                script.lua = Some(lua);
            }
            Err(error) => error!("wrath translate: the shipped script does not load: {error}"),
        }
        script
    }

    /// A fresh state: the host globals, the shipped script, then the overlay.
    fn build(&self, overlay: Option<&str>) -> Result<(Lua, Function), String> {
        let lua = Lua::new();
        install(&lua, &self.current, &self.dbcs).map_err(|error| error.to_string())?;
        lua.load(SHIPPED)
            .set_name("=wrath_translate.lua")
            .exec()
            .map_err(|error| format!("shipped script: {error}"))?;
        if let Some(text) = overlay {
            lua.load(text)
                .set_name("=wrath-translate.lua")
                .exec()
                .map_err(|error| error.to_string())?;
        }
        let hook = lua
            .globals()
            .get::<Function>("Translate")
            .map_err(|error| format!("Translate is not a function: {error}"))?;
        Ok((lua, hook))
    }

    /// Run the hook for `entity`, appending its `Set`s after `pairs` so they win.
    pub(crate) fn translate(&self, entity: &WrathEntity, pairs: &mut Vec<(u16, u32)>) {
        let Some(hook) = &self.hook else {
            return;
        };
        let (kind, window) = match entity.type_id {
            type_id::UNIT => ("unit", &entity.raw_fields),
            type_id::PLAYER => ("player", &entity.player_fields),
            type_id::GAMEOBJECT => ("gameobject", &entity.raw_fields),
            type_id::DYNAMICOBJECT => ("dynamicobject", &entity.raw_fields),
            type_id::CORPSE => ("corpse", &entity.raw_fields),
            _ => return,
        };
        {
            let mut current = self.current.borrow_mut();
            current.window = window.clone();
            current.base = std::mem::take(pairs);
            current.sets.clear();
        }
        let result = hook.call::<()>(kind);
        let mut current = self.current.borrow_mut();
        current.window = None;
        *pairs = std::mem::take(&mut current.base);
        match result {
            Ok(()) => pairs.append(&mut current.sets),
            Err(error) => {
                current.sets.clear();
                if !self.failed.replace(true) {
                    warn!(
                        "wrath translate: Translate(\"{kind}\") failed; its writes are dropped \
                         (reported once until the script reloads): {error}"
                    );
                }
            }
        }
    }
}

fn install(
    lua: &Lua,
    current: &Rc<RefCell<Current>>,
    dbcs: &Rc<RefCell<Dbcs>>,
) -> mlua::Result<()> {
    let globals = lua.globals();
    let held = current.clone();
    globals.set(
        "Raw",
        lua.create_function(move |_, index: u16| {
            Ok(held
                .borrow()
                .window
                .as_ref()
                .and_then(|window| window.get(index)))
        })?,
    )?;
    let held = current.clone();
    globals.set(
        "Get",
        lua.create_function(move |_, index: u16| Ok(held.borrow().get(index)))?,
    )?;
    let held = current.clone();
    globals.set(
        "Set",
        lua.create_function(move |_, (index, value): (u16, f64)| {
            if !value.is_finite() {
                return Err(mlua::Error::runtime(format!(
                    "Set({index}, {value}): not a number"
                )));
            }
            // Through i64 so a negative value keeps its two's-complement bits.
            held.borrow_mut().sets.push((index, value as i64 as u32));
            Ok(())
        })?,
    )?;
    let held = dbcs.clone();
    globals.set(
        "DbcRow",
        lua.create_function(move |_, (file, id): (String, u32)| {
            let row = held.borrow_mut().with(&file, |dbc| dbc.row(id));
            Ok(Variadic::from(row.unwrap_or_default()))
        })?,
    )?;
    let held = dbcs.clone();
    globals.set(
        "DbcString",
        lua.create_function(move |_, (file, offset): (String, u32)| {
            Ok(held.borrow_mut().with(&file, |dbc| dbc.string(offset)))
        })?,
    )?;
    globals.set(
        "Log",
        lua.create_function(|_, text: String| {
            info!("wrath translate: {text}");
            Ok(())
        })?,
    )?;
    Ok(())
}

/// Once a second: rebuild the script when the overlay appears, changes or goes, and ask for a
/// re-projection. Also hands the open patch chain to the DBC reader.
pub(crate) fn poll_script(
    mut script: NonSendMut<WrathTranslateScript>,
    mut map: ResMut<WrathFieldMap>,
    assets: Option<Res<benilla_assets::WorldAssets>>,
) {
    if let Some(assets) = assets {
        script.dbcs.borrow_mut().attach(&assets.chain);
    }
    let (path, overlay) = match script.file.poll() {
        FileChange::Unchanged => return,
        FileChange::Removed(path) => (path, None),
        FileChange::Changed(path, Ok(text)) => (path, Some(text)),
        FileChange::Changed(path, Err(error)) => {
            warn!("wrath translate: {} not read: {error}", path.display());
            return;
        }
    };
    match script.build(overlay.as_deref()) {
        Ok((lua, hook)) => {
            script.hook = Some(hook);
            script.lua = Some(lua);
            script.failed.set(false);
            map.reapply = true;
            if overlay.is_some() {
                info!("wrath translate: loaded {}", path.display());
            } else {
                info!(
                    "wrath translate: {} removed; shipped script only",
                    path.display()
                );
            }
        }
        Err(error) => warn!(
            "wrath translate: {} not applied, keeping the previous script: {error}",
            path.display()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dbc_row_and_string() {
        let mut bytes = b"WDBC".to_vec();
        for word in [2u32, 3, 12, 6] {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        for word in [7u32, 1, 2, 9, 3, 4] {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        bytes.extend_from_slice(b"\0Club\0");
        let dbc = Dbc::parse(bytes).expect("parses");
        assert_eq!(dbc.row(9), Some(vec![9, 3, 4]));
        assert_eq!(dbc.row(8), None);
        assert_eq!(dbc.string(1).as_deref(), Some("Club"));
    }
}
