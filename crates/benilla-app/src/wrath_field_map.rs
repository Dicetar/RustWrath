//! The live Wrath → 1.12 field table: data rules that copy delivered 3.3.5 descriptor words onto
//! 1.12 descriptor indices, so a translation fix is a text edit instead of a rebuild.
//!
//! The shipped table (`wrath_field_map.toml`, compiled in) holds the direct copies for GameObjects,
//! DynamicObjects and corpses. An optional overlay, `<benilla-config>/wrath-field-map.toml`, is
//! polled once a second; its rules run after the shipped ones, so a later rule for the same 1.12
//! index wins (and both run after the hand-written projection in `wrath.rs`, so a table rule also
//! overrides that). `replace_defaults = true` in the overlay drops the shipped rules instead.
//!
//! A reload re-projects every streamed object through the normal update path, so the change is
//! visible without a relog. A rule that is removed leaves its last written value in place until
//! the object is created again (the descriptor is merged, not rebuilt). The client never writes
//! the overlay.

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use bevy::prelude::*;
use benilla_protocol_wrath::updates::type_id;
use serde::Deserialize;

use crate::wrath::{WrathChanges, WrathEntity};
use crate::wrath_inventory::WrathFieldWindow;

/// The table compiled into the client. Edit it in the repo for a permanent fix.
const SHIPPED: &str = include_str!("wrath_field_map.toml");

/// The overlay's file name inside the config folder.
pub(crate) const OVERLAY_FILE: &str = "wrath-field-map.toml";

const POLL: Duration = Duration::from_secs(1);

/// One copy: for `i` in `0..count`, when Wrath word `from + i` was delivered, write
/// `(word >> shift) & mask` to 1.12 index `to + i`.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct FieldRule {
    pub(crate) from: u16,
    pub(crate) to: u16,
    #[serde(default)]
    pub(crate) shift: u32,
    #[serde(default = "full_mask")]
    pub(crate) mask: u32,
    #[serde(default = "one")]
    pub(crate) count: u16,
}

fn full_mask() -> u32 {
    u32::MAX
}

fn one() -> u16 {
    1
}

impl FieldRule {
    fn push(&self, window: &WrathFieldWindow, pairs: &mut Vec<(u16, u32)>) {
        for i in 0..self.count {
            if let Some(word) = window.get(self.from + i) {
                pairs.push((self.to + i, (word >> self.shift) & self.mask));
            }
        }
    }

    fn validate(&self, section: &str) -> Result<(), String> {
        if self.shift >= 32 {
            return Err(format!("[[{section}]] from = {}: shift must be below 32", self.from));
        }
        if self.count == 0 {
            return Err(format!("[[{section}]] from = {}: count must be at least 1", self.from));
        }
        if self.from.checked_add(self.count).is_none() || self.to.checked_add(self.count).is_none()
        {
            return Err(format!("[[{section}]] from = {}: range overflows", self.from));
        }
        Ok(())
    }
}

/// One table file. `unit` rules apply to creatures and players; `player` rules to players only.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct FieldTable {
    #[serde(default)]
    pub(crate) replace_defaults: bool,
    #[serde(default)]
    pub(crate) unit: Vec<FieldRule>,
    #[serde(default)]
    pub(crate) player: Vec<FieldRule>,
    #[serde(default)]
    pub(crate) gameobject: Vec<FieldRule>,
    #[serde(default)]
    pub(crate) dynamicobject: Vec<FieldRule>,
    #[serde(default)]
    pub(crate) corpse: Vec<FieldRule>,
}

impl FieldTable {
    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        let table: Self = toml::from_str(text).map_err(|error| error.to_string())?;
        for (section, rules) in [
            ("unit", &table.unit),
            ("player", &table.player),
            ("gameobject", &table.gameobject),
            ("dynamicobject", &table.dynamicobject),
            ("corpse", &table.corpse),
        ] {
            for rule in rules {
                rule.validate(section)?;
            }
        }
        Ok(table)
    }

    fn rule_count(&self) -> usize {
        self.unit.len()
            + self.player.len()
            + self.gameobject.len()
            + self.dynamicobject.len()
            + self.corpse.len()
    }

    fn push(&self, entity: &WrathEntity, pairs: &mut Vec<(u16, u32)>) {
        let none: &[FieldRule] = &[];
        let (window, sections) = match entity.type_id {
            type_id::UNIT => (entity.raw_fields.as_ref(), [self.unit.as_slice(), none]),
            type_id::PLAYER => (
                entity.player_fields.as_ref(),
                [self.unit.as_slice(), self.player.as_slice()],
            ),
            type_id::GAMEOBJECT => (entity.raw_fields.as_ref(), [self.gameobject.as_slice(), none]),
            type_id::DYNAMICOBJECT => {
                (entity.raw_fields.as_ref(), [self.dynamicobject.as_slice(), none])
            }
            type_id::CORPSE => (entity.raw_fields.as_ref(), [self.corpse.as_slice(), none]),
            _ => return,
        };
        let Some(window) = window else {
            return;
        };
        for rule in sections.into_iter().flatten() {
            rule.push(window, pairs);
        }
    }
}

/// The shipped table plus the live overlay. `reapply` asks the next projection pass to treat
/// every streamed object as updated.
#[derive(Resource)]
pub(crate) struct WrathFieldMap {
    shipped: FieldTable,
    overlay: Option<FieldTable>,
    stamp: Option<(u64, Option<SystemTime>)>,
    next_poll: Option<Instant>,
    pub(crate) reapply: bool,
}

impl Default for WrathFieldMap {
    fn default() -> Self {
        let shipped = FieldTable::parse(SHIPPED).unwrap_or_else(|error| {
            error!("wrath field map: the shipped table does not parse: {error}");
            FieldTable::default()
        });
        Self {
            shipped,
            overlay: None,
            stamp: None,
            next_poll: None,
            reapply: false,
        }
    }
}

impl WrathFieldMap {
    /// Append the table's pairs for `entity` after the projection's own, so they win.
    pub(crate) fn push_pairs(&self, entity: &WrathEntity, pairs: &mut Vec<(u16, u32)>) {
        let replace = self.overlay.as_ref().is_some_and(|o| o.replace_defaults);
        if !replace {
            self.shipped.push(entity, pairs);
        }
        if let Some(overlay) = &self.overlay {
            overlay.push(entity, pairs);
        }
    }
}

fn overlay_path() -> Option<PathBuf> {
    crate::config_dir().map(|dir| dir.join(OVERLAY_FILE))
}

/// Once a second: stat the overlay, re-read it when its size or mtime moved, and ask for a
/// re-projection. A file that does not parse is reported and the previous table stays.
pub(crate) fn poll_overlay(mut map: ResMut<WrathFieldMap>) {
    let now = Instant::now();
    if map.next_poll.is_some_and(|at| now < at) {
        return;
    }
    map.next_poll = Some(now + POLL);
    let Some(path) = overlay_path() else {
        return;
    };
    let stamp = std::fs::metadata(&path)
        .ok()
        .map(|meta| (meta.len(), meta.modified().ok()));
    if stamp == map.stamp {
        return;
    }
    map.stamp = stamp;
    if stamp.is_none() {
        if map.overlay.take().is_some() {
            info!("wrath field map: {} removed; shipped table only", path.display());
            map.reapply = true;
        }
        return;
    }
    let loaded = std::fs::read_to_string(&path)
        .map_err(|error| error.to_string())
        .and_then(|text| FieldTable::parse(&text));
    match loaded {
        Ok(table) => {
            info!(
                "wrath field map: loaded {} ({} rules{})",
                path.display(),
                table.rule_count(),
                if table.replace_defaults {
                    ", replacing the shipped table"
                } else {
                    ""
                }
            );
            if map.overlay.as_ref() != Some(&table) {
                map.overlay = Some(table);
                map.reapply = true;
            }
        }
        Err(error) => warn!(
            "wrath field map: {} not applied, keeping the previous table: {error}",
            path.display()
        ),
    }
}

/// Queue every object the table can touch as updated, so a reload reaches objects the server is
/// not currently updating. The update path diffs the descriptor, so untouched objects raise
/// nothing.
pub(crate) fn queue_reapply(
    changes: &mut WrathChanges,
    entities: &std::collections::BTreeMap<u64, WrathEntity>,
) {
    for (guid, entity) in entities {
        if !matches!(
            entity.type_id,
            type_id::UNIT
                | type_id::PLAYER
                | type_id::GAMEOBJECT
                | type_id::DYNAMICOBJECT
                | type_id::CORPSE
        ) {
            continue;
        }
        if changes.created.contains(guid)
            || changes.updated.contains(guid)
            || changes.removed.contains(guid)
        {
            continue;
        }
        changes.updated.push(*guid);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_table_parses() {
        let table = FieldTable::parse(SHIPPED).expect("shipped table");
        assert!(!table.gameobject.is_empty());
    }

    #[test]
    fn rule_copies_only_delivered_words() {
        let mut values = vec![0u32; 18];
        values[17] = 0x0403_0201;
        let mut delivered = vec![0u64; 1];
        delivered[0] |= 1 << 17;
        let window = WrathFieldWindow::new(values, delivered);
        let rule = FieldRule { from: 17, to: 21, shift: 8, mask: 0xff, count: 1 };
        let mut pairs = Vec::new();
        rule.push(&window, &mut pairs);
        FieldRule { from: 16, to: 22, shift: 0, mask: u32::MAX, count: 1 }.push(&window, &mut pairs);
        assert_eq!(pairs, vec![(21, 0x02)]);
    }
}
