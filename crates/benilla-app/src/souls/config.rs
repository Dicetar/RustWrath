//! `benilla-config/combat/souls.toml`: one switch per Souls feature and the binds, re-read each
//! time `F10` turns Souls combat on, so a change takes effect without a restart.
//! The equipped main-hand subclass selects combat/kits/<name>.kit (scripts/combat-kits.csv);
//! missing kits fall back to combat/player.kit, then stock. Weapon changes re-read the kit too.
//!
//! ```toml
//! [features]          # every switch defaults to true; a missing key keeps its default
//! root_motion = true  # actions move the body by the kit's root motion
//! turn_lock = true    # the kit's no-turn windows hold the facing
//! lock_on = true      # the lock-on key holds the facing on a target; A/D circle it
//! lock_targets = true # the lock is the WoW target (target frame, Tab); a Tab or click onto
//!                     # another attackable unit moves the lock to it
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
//! two_hand = true     # the grip key holds a one-handed weapon in both hands (its two-handed
//!                     # kit, kits/<name>-twohand.kit, and the server's two-hand bonus)
//! server = true       # talk to the server script (integrations/ale/rustwrath_souls.lua): hits
//!                     # are reported for Souls damage, sprint raises the run speed
//! enemy_strikes = true # judge the server's telegraphed enemy strikes (dodge, guard, parry)
//! retarget = true     # kit actions pose the player's rig from the Elden Ring clip each row was
//!                     # read from (benilla-config/combat/player_anims.bin)
//! retarget_locomotion = true # idle, guard, walk, run, sprint, crouch and airborne play the
//!                     # Elden Ring locomotion clips too
//! er_controls = true  # the Elden Ring layout below takes its keys and buttons from the WoW
//!                     # bindings; the mouse orbits with the cursor hidden, the cursor key
//!                     # (LeftCtrl) brings the cursor up for the action bars and panels
//! free_camera = true  # the camera orbits freely, movement is camera-relative and the body
//!                     # faces where it runs (off: the orbit turns the body, as mouselook does)
//!
//! [binds]             # key names: A-Z, 0-9, F1-F12, Space, Tab, Backquote, Escape, Enter,
//! light = "MouseLeft" # LeftShift, LeftCtrl, LeftAlt, CapsLock, MouseLeft, MouseRight,
//! heavy = "Shift+MouseLeft" # MouseMiddle, Mouse4, Mouse5 (BUTTON4/BUTTON5); the combat binds
//! guard = "MouseRight" # take a "Shift+" prefix
//! parry = "Shift+MouseRight"
//! grip = "G"          # one hand / both hands (two_hand)
//! left = "Mouse5"
//! dodge = "Space"     # tap rolls, hold sprints
//! lock_on = "Q"
//! lock_on_alt = "MouseMiddle"
//! crouch = "X"
//! jump = "F"          # runs the JUMP binding
//! interact = "E"      # opens, loots or talks to what is in front
//! use_item = "R"      # runs the `quick_item` binding
//! walk = "LeftAlt"    # hold to walk (TOGGLERUN on press and release)
//! cursor = "LeftCtrl" # tap: the cursor up for the UI, tap again: back to the orbit
//!
//! [controls]
//! quick_item = "ACTIONBUTTON12" # the WoW binding the use-item key runs
//! cursor_keys = ["Escape", "B", "C", "P", "M", "L", "N", "O", "I", "K", "U", "H", "J"]
//!                     # keys that open a panel: the cursor comes up with it, and a movement key
//!                     # puts it away again
//! ```
//!
//! The raw keys run through the binding dispatch's typing gate (a focused EditBox eats them).
//! With `er_controls` on, a Souls key's own WoW binding does not run (Space no longer jumps, Q
//! and E no longer strafe: A and D move left and right against the camera), and the mouse's
//! primary buttons are the attacks while the camera orbits. With the cursor up they are the
//! stock buttons again, so the action bars and every panel work as ever. With `er_controls` off
//! a Souls key also runs its WoW binding, and mouse binds act beside the stock clicks.

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
    pub(crate) lock_targets: bool,
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
    pub(crate) two_hand: bool,
    pub(crate) server: bool,
    pub(crate) enemy_strikes: bool,
    pub(crate) retarget: bool,
    pub(crate) retarget_locomotion: bool,
    pub(crate) er_controls: bool,
    pub(crate) free_camera: bool,
}

impl Default for SoulsFeatures {
    fn default() -> Self {
        Self {
            root_motion: true,
            turn_lock: true,
            lock_on: true,
            lock_targets: true,
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
            two_hand: true,
            server: true,
            enemy_strikes: true,
            retarget: true,
            retarget_locomotion: true,
            er_controls: true,
            free_camera: true,
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
    lock_on_alt: String,
    crouch: String,
    left: String,
    parry: String,
    grip: String,
    jump: String,
    interact: String,
    use_item: String,
    walk: String,
    cursor: String,
}

impl Default for BindNames {
    fn default() -> Self {
        Self {
            light: "MouseLeft".into(),
            heavy: "Shift+MouseLeft".into(),
            dodge: "Space".into(),
            guard: "MouseRight".into(),
            lock_on: "Q".into(),
            lock_on_alt: "MouseMiddle".into(),
            crouch: "X".into(),
            left: "Mouse5".into(),
            parry: "Shift+MouseRight".into(),
            grip: "G".into(),
            jump: "F".into(),
            interact: "E".into(),
            use_item: "R".into(),
            walk: "LeftAlt".into(),
            cursor: "LeftCtrl".into(),
        }
    }
}

/// The `[controls]` section as written.
#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
struct ControlNames {
    quick_item: String,
    cursor_keys: Vec<String>,
}

impl Default for ControlNames {
    fn default() -> Self {
        Self {
            quick_item: "ACTIONBUTTON12".into(),
            cursor_keys: [
                "Escape", "B", "C", "P", "M", "L", "N", "O", "I", "K", "U", "H", "J",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct SoulsFile {
    features: SoulsFeatures,
    binds: BindNames,
    controls: ControlNames,
}

/// A key or mouse button a Souls bind names.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SoulsKey {
    Key(KeyCode),
    Mouse(MouseButton),
}

/// A combat bind: a key or button, with Shift held or not. A plain bind whose key also has a
/// Shift bind waits for Shift up ([`super::input::KeyRead`]), so `MouseLeft` and
/// `Shift+MouseLeft` are two presses.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct SoulsBind {
    pub(crate) key: SoulsKey,
    pub(crate) shift: bool,
}

impl SoulsBind {
    pub(crate) const fn plain(key: SoulsKey) -> Self {
        Self { key, shift: false }
    }

    pub(crate) const fn shifted(key: SoulsKey) -> Self {
        Self { key, shift: true }
    }
}

/// The resolved binds. The combat binds may carry Shift; the rest are bare keys.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct SoulsBinds {
    pub(crate) light: SoulsBind,
    pub(crate) heavy: SoulsBind,
    pub(crate) dodge: SoulsBind,
    pub(crate) guard: SoulsBind,
    pub(crate) lock_on: SoulsBind,
    pub(crate) lock_on_alt: SoulsBind,
    pub(crate) crouch: SoulsBind,
    pub(crate) left: SoulsBind,
    pub(crate) parry: SoulsBind,
    /// One hand or both: [`crate::combat_kit::PlayerKit::toggle_grip`].
    pub(crate) grip: SoulsBind,
    /// Runs the `JUMP` binding while the layout is on.
    pub(crate) jump: SoulsKey,
    /// Interacts with what is in front ([`super::controls`]).
    pub(crate) interact: SoulsKey,
    /// Runs [`SoulsControls::quick_item`].
    pub(crate) use_item: SoulsKey,
    /// Hold to walk: `TOGGLERUN` on its press and on its release.
    pub(crate) walk: SoulsKey,
    /// Tap to bring the cursor up for the UI, tap again to orbit.
    pub(crate) cursor: SoulsKey,
}

impl SoulsBinds {
    /// The combat binds, for the Shift pairing and the claims.
    pub(crate) fn combat(&self) -> [SoulsBind; 10] {
        [
            self.light,
            self.heavy,
            self.dodge,
            self.guard,
            self.lock_on,
            self.lock_on_alt,
            self.crouch,
            self.left,
            self.parry,
            self.grip,
        ]
    }
}

/// The `[controls]` section, resolved.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct SoulsControls {
    /// The WoW binding the use-item key runs (by default the twelfth action button's).
    pub(crate) quick_item: String,
    /// Keys that open a panel: the cursor comes up with them.
    pub(crate) cursor_keys: Vec<KeyCode>,
}

/// The live Souls configuration.
#[derive(Resource, Clone, PartialEq, Eq, Debug)]
pub(crate) struct SoulsConfig {
    pub(crate) features: SoulsFeatures,
    pub(crate) binds: SoulsBinds,
    pub(crate) controls: SoulsControls,
}

impl Default for SoulsConfig {
    fn default() -> Self {
        use SoulsKey::{Key, Mouse};
        resolve(SoulsFile::default()).unwrap_or(Self {
            features: SoulsFeatures::default(),
            binds: SoulsBinds {
                light: SoulsBind::plain(Mouse(MouseButton::Left)),
                heavy: SoulsBind::shifted(Mouse(MouseButton::Left)),
                dodge: SoulsBind::plain(Key(KeyCode::Space)),
                guard: SoulsBind::plain(Mouse(MouseButton::Right)),
                lock_on: SoulsBind::plain(Key(KeyCode::KeyQ)),
                lock_on_alt: SoulsBind::plain(Mouse(MouseButton::Middle)),
                crouch: SoulsBind::plain(Key(KeyCode::KeyX)),
                left: SoulsBind::plain(Mouse(MouseButton::Back)),
                parry: SoulsBind::shifted(Mouse(MouseButton::Right)),
                grip: SoulsBind::plain(Key(KeyCode::KeyG)),
                jump: Key(KeyCode::KeyF),
                interact: Key(KeyCode::KeyE),
                use_item: Key(KeyCode::KeyR),
                walk: Key(KeyCode::AltLeft),
                cursor: Key(KeyCode::ControlLeft),
            },
            controls: SoulsControls {
                quick_item: "ACTIONBUTTON12".into(),
                cursor_keys: vec![KeyCode::Escape],
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
                warn!(
                    "souls config: {} could not be read: {error}",
                    path.display()
                );
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
    let bind = |what: &str, name: &str| {
        parse_bind(name).ok_or_else(|| format!("binds.{what}: unknown key `{name}`"))
    };
    // The keys the layout hands to a WoW binding go through the keyboard dispatch.
    let keyboard = |what: &str, name: &str| match key(what, name)? {
        SoulsKey::Key(code) => Ok(SoulsKey::Key(code)),
        SoulsKey::Mouse(_) => Err(format!("binds.{what}: `{name}` must be a keyboard key")),
    };
    let names = &file.binds;
    let cursor_keys = file
        .controls
        .cursor_keys
        .iter()
        .map(|name| match parse_key(name) {
            Some(SoulsKey::Key(code)) => Ok(code),
            _ => Err(format!(
                "controls.cursor_keys: unknown keyboard key `{name}`"
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let quick_item = file.controls.quick_item.trim().to_ascii_uppercase();
    if quick_item.is_empty() {
        return Err("controls.quick_item: empty binding name".into());
    }
    Ok(SoulsConfig {
        features: file.features,
        binds: SoulsBinds {
            light: bind("light", &names.light)?,
            heavy: bind("heavy", &names.heavy)?,
            dodge: bind("dodge", &names.dodge)?,
            guard: bind("guard", &names.guard)?,
            lock_on: bind("lock_on", &names.lock_on)?,
            lock_on_alt: bind("lock_on_alt", &names.lock_on_alt)?,
            crouch: bind("crouch", &names.crouch)?,
            left: bind("left", &names.left)?,
            parry: bind("parry", &names.parry)?,
            grip: bind("grip", &names.grip)?,
            jump: keyboard("jump", &names.jump)?,
            interact: key("interact", &names.interact)?,
            use_item: keyboard("use_item", &names.use_item)?,
            walk: keyboard("walk", &names.walk)?,
            cursor: keyboard("cursor", &names.cursor)?,
        },
        controls: SoulsControls {
            quick_item,
            cursor_keys,
        },
    })
}

/// A combat bind's name: a key name, optionally after `Shift+`.
pub(crate) fn parse_bind(name: &str) -> Option<SoulsBind> {
    let trimmed = name.trim();
    let (shift, rest) = match trimmed.split_once('+') {
        Some((modifier, rest)) if modifier.trim().eq_ignore_ascii_case("shift") => (true, rest),
        Some(_) => return None,
        None => (false, trimmed),
    };
    let key = parse_key(rest)?;
    // Shift is no chord with itself.
    if shift && matches!(key, SoulsKey::Key(KeyCode::ShiftLeft | KeyCode::ShiftRight)) {
        return None;
    }
    Some(SoulsBind { key, shift })
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
        "ESCAPE" | "ESC" => K::Escape,
        "ENTER" | "RETURN" => K::Enter,
        "RIGHTSHIFT" => K::ShiftRight,
        "RIGHTCTRL" => K::ControlRight,
        "RIGHTALT" => K::AltRight,
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
        assert_eq!(
            config.binds.heavy,
            SoulsBind::plain(SoulsKey::Key(KeyCode::KeyG))
        );
        assert_eq!(
            config.binds.guard,
            SoulsBind::plain(SoulsKey::Mouse(MouseButton::Back))
        );
        assert_eq!(
            config.binds.light,
            SoulsBind::plain(SoulsKey::Mouse(MouseButton::Left))
        );
        assert_eq!(
            config.binds.parry,
            SoulsBind::shifted(SoulsKey::Mouse(MouseButton::Right))
        );
        assert_eq!(config.controls.cursor_keys.first(), Some(&KeyCode::Escape));
        assert_eq!(
            SoulsConfig::from_text("").expect("empty"),
            SoulsConfig::default()
        );

        let bad_key = SoulsConfig::from_text("[binds]\nlight = \"F10\"\n").unwrap_err();
        assert!(bad_key.contains("F10"), "{bad_key}");
        assert!(SoulsConfig::from_text("[features]\nfly = true\n").is_err());
        assert!(SoulsConfig::from_text("[binds]\nlight = \"Ctrl+E\"\n").is_err());
        let mouse_jump = SoulsConfig::from_text("[binds]\njump = \"MouseLeft\"\n").unwrap_err();
        assert!(mouse_jump.contains("keyboard"), "{mouse_jump}");
    }
}
