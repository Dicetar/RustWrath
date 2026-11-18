//! `benilla-config/combat/souls.toml`: one switch per Souls feature and the binds, re-read each
//! time `F10` turns Souls combat on, so a change takes effect without a restart.
//!
//! ```toml
//! [features]          # every switch defaults to true; a missing key keeps its default
//! root_motion = true  # actions move the body by the kit's root motion
//! turn_lock = true    # the kit's no-turn windows hold the facing
//! lock_on = true      # the lock-on key holds the facing on a target; A/D circle it
//! heavy = true        # the heavy key runs the heavy chain
//! charge = true       # holding heavy charges; releasing in the window swings uncharged
//! sprint = true       # holding dodge while moving sprints (and drains stamina)
//! running_attacks = true # light/heavy while sprinting are RunLight/RunHeavy
//! guard = true        # holding guard absorbs hits for stamina instead of being staggered
//! guard_counter = true # heavy right after a guarded hit is the guard counter
//! iframes = true      # a hit inside a dodge's i-frames does not stagger
//! hits = true         # the kit's hit windows test the blade's reach against attackable units
//! hit_stop = true     # a landed hit freezes the action briefly and jolts the camera
//! engage_on_hit = true # a landed hit starts stock melee on the target (without the server script)
//! hud = true          # stamina bar, mode chip and lock-on marker
//! turn_rate = true    # the kit's turn-rate windows cap how fast an attack turns
//! poise = true        # hits take poise before they stagger; heavy windups halve it (hyperarmor)
//! crouch = true       # the crouch key crouches; light from a crouch is the crouch attack
//! jump_attacks = true # light/heavy in the air are the kit's jump attacks, when it has them
//! left_hand = true    # the left key runs the kit's left-hand chain, when it has one
//! parry = true        # the parry key runs the kit's Parry, when it has one
//! server = true       # talk to the server script (integrations/ale/rustwrath_souls.lua): hits
//!                     # are reported for Souls damage, sprint raises the run speed
//! enemy_strikes = true # judge the server's telegraphed enemy strikes (dodge, guard, parry)
//! retarget = true     # kit actions pose the player's rig from the Elden Ring clip each row was
//!                     # read from (benilla-config/combat/player_anims.bin)
//! retarget_locomotion = true # idle, guard, walk, run, sprint, crouch and airborne play the
//!                     # Elden Ring locomotion clips too
//!
//! [binds]             # key names: A-Z, 0-9, F1-F12, Space, Tab, Backquote, LeftShift,
//! light = "F9"        # LeftCtrl, LeftAlt, CapsLock, MouseLeft, MouseRight, MouseMiddle,
//! heavy = "F8"        # Mouse4, Mouse5 (BUTTON4/BUTTON5)
//! dodge = "V"
//! guard = "F7"
//! lock_on = "MouseMiddle"
//! crouch = "F6"
//! left = "F11"
//! parry = "F12"
//! ```
//!
//! The raw keys run through the binding dispatch's typing gate (a focused EditBox eats them), but
//! not its binding table: a Souls key that also has a WoW binding does both. The defaults are keys
//! the stock bindings leave free, except `V` (the nameplate toggle), kept from the first slice.

use bevy::prelude::*;
use serde::Deserialize;

/// The feature switches. Each one only acts while Souls combat is on (`F10`); off, the feature
/// leaves the stock path untouched.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct SoulsFeatures {
    pub(crate) root_motion: bool,
    pub(crate) turn_lock: bool,
    pub(crate) lock_on: bool,
    pub(crate) heavy: bool,
    pub(crate) charge: bool,
    pub(crate) sprint: bool,
    pub(crate) running_attacks: bool,
    pub(crate) guard: bool,
    pub(crate) guard_counter: bool,
    pub(crate) iframes: bool,
    pub(crate) hits: bool,
    pub(crate) hit_stop: bool,
    pub(crate) engage_on_hit: bool,
    pub(crate) hud: bool,
    pub(crate) turn_rate: bool,
    pub(crate) poise: bool,
    pub(crate) crouch: bool,
    pub(crate) jump_attacks: bool,
    pub(crate) left_hand: bool,
    pub(crate) parry: bool,
    pub(crate) server: bool,
    pub(crate) enemy_strikes: bool,
    pub(crate) retarget: bool,
    pub(crate) retarget_locomotion: bool,
}

impl Default for SoulsFeatures {
    fn default() -> Self {
        Self {
            root_motion: true,
            turn_lock: true,
            lock_on: true,
            heavy: true,
            charge: true,
            sprint: true,
            running_attacks: true,
            guard: true,
            guard_counter: true,
            iframes: true,
            hits: true,
            hit_stop: true,
            engage_on_hit: true,
            hud: true,
            turn_rate: true,
            poise: true,
            crouch: true,
            jump_attacks: true,
            left_hand: true,
            parry: true,
            server: true,
            enemy_strikes: true,
            retarget: true,
            retarget_locomotion: true,
        }
    }
}

/// The binds as written in the file.
#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
struct BindNames {
    light: String,
    heavy: String,
    dodge: String,
    guard: String,
    lock_on: String,
    crouch: String,
    left: String,
    parry: String,
}

impl Default for BindNames {
    fn default() -> Self {
        Self {
            light: "F9".into(),
            heavy: "F8".into(),
            dodge: "V".into(),
            guard: "F7".into(),
            lock_on: "MouseMiddle".into(),
            crouch: "F6".into(),
            left: "F11".into(),
            parry: "F12".into(),
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct SoulsFile {
    features: SoulsFeatures,
    binds: BindNames,
}

/// A key or mouse button a Souls bind names.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SoulsKey {
    Key(KeyCode),
    Mouse(MouseButton),
}

/// The resolved binds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct SoulsBinds {
    pub(crate) light: SoulsKey,
    pub(crate) heavy: SoulsKey,
    pub(crate) dodge: SoulsKey,
    pub(crate) guard: SoulsKey,
    pub(crate) lock_on: SoulsKey,
    pub(crate) crouch: SoulsKey,
    pub(crate) left: SoulsKey,
    pub(crate) parry: SoulsKey,
}

/// The live Souls configuration.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct SoulsConfig {
    pub(crate) features: SoulsFeatures,
    pub(crate) binds: SoulsBinds,
}

impl Default for SoulsConfig {
    fn default() -> Self {
        resolve(SoulsFile::default()).unwrap_or(Self {
            features: SoulsFeatures::default(),
            binds: SoulsBinds {
                light: SoulsKey::Key(KeyCode::F9),
                heavy: SoulsKey::Key(KeyCode::F8),
                dodge: SoulsKey::Key(KeyCode::KeyV),
                guard: SoulsKey::Key(KeyCode::F7),
                lock_on: SoulsKey::Mouse(MouseButton::Middle),
                crouch: SoulsKey::Key(KeyCode::F6),
                left: SoulsKey::Key(KeyCode::F11),
                parry: SoulsKey::Key(KeyCode::F12),
            },
        })
    }
}

impl SoulsConfig {
    /// Parse the file's text; an unknown key name, switch or section is an error naming it.
    pub(crate) fn from_text(text: &str) -> Result<Self, String> {
        let file: SoulsFile = toml::from_str(text).map_err(|error| error.to_string())?;
        resolve(file)
    }

    /// Read the file again. No file is the defaults; a file that does not parse keeps the
    /// current configuration and says why.
    pub(crate) fn reload(&mut self) {
        let Some(path) = crate::local_state::souls_config_path() else {
            return;
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                *self = Self::default();
                info!(
                    "souls config: none at {}; every feature on, default binds",
                    path.display()
                );
                return;
            }
            Err(error) => {
                warn!("souls config: {} could not be read: {error}", path.display());
                return;
            }
        };
        match Self::from_text(&text) {
            Ok(config) => {
                *self = config;
                info!("souls config: {} — {:?}", path.display(), self.features);
            }
            Err(error) => warn!(
                "souls config: {} is not usable, keeping the current one: {error}",
                path.display()
            ),
        }
    }
}

/// The startup read.
pub(crate) fn load_souls_config(mut config: ResMut<SoulsConfig>) {
    config.reload();
}

fn resolve(file: SoulsFile) -> Result<SoulsConfig, String> {
    let key = |what: &str, name: &str| {
        parse_key(name).ok_or_else(|| format!("binds.{what}: unknown key `{name}`"))
    };
    Ok(SoulsConfig {
        features: file.features,
        binds: SoulsBinds {
            light: key("light", &file.binds.light)?,
            heavy: key("heavy", &file.binds.heavy)?,
            dodge: key("dodge", &file.binds.dodge)?,
            guard: key("guard", &file.binds.guard)?,
            lock_on: key("lock_on", &file.binds.lock_on)?,
            crouch: key("crouch", &file.binds.crouch)?,
            left: key("left", &file.binds.left)?,
            parry: key("parry", &file.binds.parry)?,
        },
    })
}

/// A bind's key name, case-insensitive.
pub(crate) fn parse_key(name: &str) -> Option<SoulsKey> {
    use KeyCode as K;
    let upper = name.trim().to_ascii_uppercase();
    let key = match upper.as_str() {
        "A" => K::KeyA,
        "B" => K::KeyB,
        "C" => K::KeyC,
        "D" => K::KeyD,
        "E" => K::KeyE,
        "F" => K::KeyF,
        "G" => K::KeyG,
        "H" => K::KeyH,
        "I" => K::KeyI,
        "J" => K::KeyJ,
        "K" => K::KeyK,
        "L" => K::KeyL,
        "M" => K::KeyM,
        "N" => K::KeyN,
        "O" => K::KeyO,
        "P" => K::KeyP,
        "Q" => K::KeyQ,
        "R" => K::KeyR,
        "S" => K::KeyS,
        "T" => K::KeyT,
        "U" => K::KeyU,
        "V" => K::KeyV,
        "W" => K::KeyW,
        "X" => K::KeyX,
        "Y" => K::KeyY,
        "Z" => K::KeyZ,
        "0" => K::Digit0,
        "1" => K::Digit1,
        "2" => K::Digit2,
        "3" => K::Digit3,
        "4" => K::Digit4,
        "5" => K::Digit5,
        "6" => K::Digit6,
        "7" => K::Digit7,
        "8" => K::Digit8,
        "9" => K::Digit9,
        "F1" => K::F1,
        "F2" => K::F2,
        "F3" => K::F3,
        "F4" => K::F4,
        "F5" => K::F5,
        "F6" => K::F6,
        "F7" => K::F7,
        "F8" => K::F8,
        "F9" => K::F9,
        // F10 is the mode toggle itself.
        "F11" => K::F11,
        "F12" => K::F12,
        "SPACE" => K::Space,
        "TAB" => K::Tab,
        "BACKQUOTE" | "`" => K::Backquote,
        "LEFTSHIFT" | "SHIFT" => K::ShiftLeft,
        "LEFTCTRL" | "CTRL" => K::ControlLeft,
        "LEFTALT" | "ALT" => K::AltLeft,
        "CAPSLOCK" => K::CapsLock,
        "MOUSELEFT" | "BUTTON1" => return Some(SoulsKey::Mouse(MouseButton::Left)),
        "MOUSERIGHT" | "BUTTON2" => return Some(SoulsKey::Mouse(MouseButton::Right)),
        "MOUSEMIDDLE" | "BUTTON3" => return Some(SoulsKey::Mouse(MouseButton::Middle)),
        // The bindings map BUTTON4 to winit's `Forward` and BUTTON5 to `Back`.
        "MOUSE4" | "BUTTON4" => return Some(SoulsKey::Mouse(MouseButton::Forward)),
        "MOUSE5" | "BUTTON5" => return Some(SoulsKey::Mouse(MouseButton::Back)),
        _ => return None,
    };
    Some(SoulsKey::Key(key))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A partial file keeps every default it does not name; a bad key name or switch is refused
    /// with the name.
    #[test]
    fn partial_file_keeps_defaults_and_bad_names_are_refused() {
        let config = SoulsConfig::from_text(
            "[features]\nroot_motion = false\n[binds]\nheavy = \"g\"\nguard = \"Mouse5\"\n",
        )
        .expect("a valid file");
        assert!(!config.features.root_motion);
        assert!(config.features.lock_on);
        assert_eq!(config.binds.heavy, SoulsKey::Key(KeyCode::KeyG));
        assert_eq!(config.binds.guard, SoulsKey::Mouse(MouseButton::Back));
        assert_eq!(config.binds.light, SoulsKey::Key(KeyCode::F9));
        assert_eq!(SoulsConfig::from_text("").expect("empty"), SoulsConfig::default());

        let bad_key = SoulsConfig::from_text("[binds]\nlight = \"F10\"\n").unwrap_err();
        assert!(bad_key.contains("F10"), "{bad_key}");
        assert!(SoulsConfig::from_text("[features]\nfly = true\n").is_err());
    }
}
