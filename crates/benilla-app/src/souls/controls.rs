//! The Elden Ring control layout's claims ([`ErControls`]), which the core's input readers
//! (the bindings dispatch, the world mouse, the UI cursor feed) stand aside for. The layout's
//! systems are `benilla-souls`'s `controls`.

use bevy::prelude::*;

use super::config::SoulsKey;
use super::SoulsConfig;

/// What the ER layout claims this frame, for the readers that must stand aside: the bindings
/// dispatch, the world mouse, the UI cursor feed and the Souls key reads.
#[derive(Resource, Default, Debug)]
pub struct ErControls {
    pub active: bool,
    /// The free camera is on (`free_camera`).
    pub free_camera: bool,
    /// The camera orbits with the cursor hidden; false while the cursor is up.
    pub orbit: bool,
    /// The cursor came up on its own (a panel key, an interaction): a movement key puts it away.
    pub auto_cursor: bool,
    /// The cursor key is down and nothing else was pressed with it: its release is a tap.
    pub cursor_tap: bool,
    /// The keys whose WoW bindings do not run.
    pub keys: Vec<KeyCode>,
    /// Claimed keys that run another binding: `(key, binding, again on the release)`.
    pub remap: Vec<(KeyCode, String, bool)>,
    /// The mouse buttons beside the primary two that the Souls binds take.
    pub mouse: Vec<MouseButton>,
    /// Keys a `Shift+` combat bind names: their plain binds wait for Shift up. Kept while Souls
    /// is on, the layout on or not.
    pub shifted: Vec<SoulsKey>,
}

impl ErControls {
    /// The layout is on: Souls on, its switch on, in the world.
    pub fn active(&self) -> bool {
        self.active
    }

    /// The cursor is hidden for the orbit.
    pub fn hides_cursor(&self) -> bool {
        self.active && self.orbit
    }

    /// `key`'s own WoW binding does not run.
    pub fn claims_key(&self, key: KeyCode) -> bool {
        self.active && self.keys.contains(&key)
    }

    /// `button` is the layout's: the primary two while the camera orbits, the others when a Souls
    /// bind names them.
    pub fn claims_mouse(&self, button: MouseButton) -> bool {
        self.active
            && match button {
                MouseButton::Left | MouseButton::Right => self.orbit,
                other => self.mouse.contains(&other),
            }
    }

    /// The binding a claimed `key` runs instead, and whether its release runs it again.
    pub fn remap(&self, key: KeyCode) -> Option<(&str, bool)> {
        if !self.active {
            return None;
        }
        self.remap
            .iter()
            .find(|(k, ..)| *k == key)
            .map(|(_, command, again)| (command.as_str(), *again))
    }

    /// Bring the cursor up on its own: the next movement key puts it away.
    pub fn summon_cursor(&mut self) {
        if self.orbit {
            self.orbit = false;
            self.auto_cursor = true;
        }
    }

    /// Fill the claims from the binds.
    pub fn claim(&mut self, config: &SoulsConfig) {
        let binds = &config.binds;
        self.keys.clear();
        self.mouse.clear();
        self.remap.clear();
        let combat = binds.combat().map(|bind| bind.key);
        let others = [binds.jump, binds.interact, binds.use_item, binds.walk];
        for key in combat.into_iter().chain(others) {
            match key {
                SoulsKey::Key(code) => {
                    if !self.keys.contains(&code) {
                        self.keys.push(code);
                    }
                }
                SoulsKey::Mouse(MouseButton::Left | MouseButton::Right) => {}
                SoulsKey::Mouse(button) => {
                    if !self.mouse.contains(&button) {
                        self.mouse.push(button);
                    }
                }
            }
        }
        let remaps = [
            (binds.jump, "JUMP".to_string(), false),
            (binds.use_item, config.controls.quick_item.clone(), false),
            (binds.walk, "TOGGLERUN".to_string(), true),
        ];
        for (key, command, again) in remaps {
            if let SoulsKey::Key(code) = key {
                self.remap.push((code, command, again));
            }
        }
    }
}
