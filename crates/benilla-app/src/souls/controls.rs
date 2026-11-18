//! The Elden Ring control layout ([`ErControls`]), on the `er_controls` switch while Souls combat
//! is on (`F10`):
//!
//! - **The orbit.** The cursor is hidden and the mouse orbits the camera (the free camera,
//!   `free_camera`; off, the orbit turns the body as right-button mouselook does). The left and
//!   right buttons are the Souls attacks and the guard: the world neither looks nor clicks with
//!   them ([`crate::player::camera`] drops their presses, the bindings skip `BUTTON1`/`BUTTON2`).
//! - **The cursor key** (`LeftCtrl`), tapped alone, brings the cursor up: the buttons are the stock
//!   ones again, so the action bars, the panels and a world click work as ever. Tapped again, back
//!   to the orbit. A panel key (`[controls] cursor_keys`) and an interaction bring the cursor up
//!   too, and then the next movement key puts it away.
//! - **The layout's keys** take over their WoW bindings: a claimed key runs no binding of its own
//!   (Space no longer jumps, Q and E no longer strafe), and a remapped one runs the binding the
//!   layout names (`jump` runs `JUMP`, `use_item` the quick-item binding, `walk` `TOGGLERUN` on
//!   its press and its release). The number keys, and every key the layout does not name, keep
//!   their bindings, so the hotbar casts without the cursor.
//! - **Interact** (`E`): the selection when it is in reach and can be used, else the nearest
//!   usable thing in front (a lootable corpse, a chest, a door, a mailbox, a friendly NPC), as a
//!   right-click on it would.
//!
//! Off (the switch, the mode or the world), nothing is claimed and the cursor is the stock one.

use bevy::prelude::*;
use benilla_protocol::messages::ObjectType;

use super::config::SoulsKey;
use super::targets::{attackable, off_facing};
use super::{souls_on, SoulsConfig};
use crate::bindings::{BindingsState, Input};
use crate::client_state::ClientState;
use crate::combat_mode::CombatModeView;
use crate::net::{Guid, ObjectStore, SelfPlayer};
use crate::player::camera::FlyCam;
use crate::player::Player;
use crate::target::click::Interact;
use crate::target::{RelationWorld, Selection};
use crate::ui_script::UiKeyboardCapture;

/// Interaction reaches this far, yards (the stock interact range is about five).
const INTERACT_YARDS: f32 = 5.0;
/// ...and this far above or below.
const INTERACT_HEIGHT_YARDS: f32 = 3.0;
/// "In front" for the interact pick: within this many degrees of the facing.
const INTERACT_FRONT_DEGREES: f32 = 100.0;
/// Closer than this, the pick ignores the angle (the thing is underfoot).
const INTERACT_UNDERFOOT_YARDS: f32 = 1.5;
/// Yards of distance one radian off the facing costs the interact pick.
const INTERACT_ANGLE_COST: f32 = 2.0;
/// `UNIT_DYNFLAG_LOOTABLE`.
const DYNFLAG_LOOTABLE: u32 = 0x1;
/// `GO_FLAG_NOT_SELECTABLE`: no interact.
const GO_FLAG_NO_INTERACT: u32 = 0x10;
/// The GameObject types a press uses: door, button, quest giver, chest, chair, text, goober,
/// fishing node, ritual, mailbox, spell caster, meeting stone, flag stand, fishing hole, flag
/// drop, barber chair.
const USABLE_GO_TYPES: [i32; 16] = [0, 1, 2, 3, 7, 9, 10, 17, 18, 19, 22, 23, 24, 25, 26, 32];
/// Of those, the ones that open a window: quest giver, chest, text, goober, fishing node,
/// mailbox, fishing hole.
const WINDOW_GO_TYPES: [i32; 7] = [2, 3, 9, 10, 17, 19, 25];

/// What the ER layout claims this frame, for the readers that must stand aside: the bindings
/// dispatch, the world mouse, the UI cursor feed and the Souls key reads.
#[derive(Resource, Default, Debug)]
pub(crate) struct ErControls {
    active: bool,
    /// The free camera is on (`free_camera`).
    pub(super) free_camera: bool,
    /// The camera orbits with the cursor hidden; false while the cursor is up.
    orbit: bool,
    /// The cursor came up on its own (a panel key, an interaction): a movement key puts it away.
    auto_cursor: bool,
    /// The cursor key is down and nothing else was pressed with it: its release is a tap.
    cursor_tap: bool,
    /// The keys whose WoW bindings do not run.
    keys: Vec<KeyCode>,
    /// Claimed keys that run another binding: `(key, binding, again on the release)`.
    remap: Vec<(KeyCode, String, bool)>,
    /// The mouse buttons beside the primary two that the Souls binds take.
    mouse: Vec<MouseButton>,
    /// Keys a `Shift+` combat bind names: their plain binds wait for Shift up. Kept while Souls
    /// is on, the layout on or not.
    pub(super) shifted: Vec<SoulsKey>,
}

impl ErControls {
    /// The layout is on: Souls on, its switch on, in the world.
    pub(crate) fn active(&self) -> bool {
        self.active
    }

    /// The cursor is hidden for the orbit.
    pub(crate) fn hides_cursor(&self) -> bool {
        self.active && self.orbit
    }

    /// `key`'s own WoW binding does not run.
    pub(crate) fn claims_key(&self, key: KeyCode) -> bool {
        self.active && self.keys.contains(&key)
    }

    /// `button` is the layout's: the primary two while the camera orbits, the others when a Souls
    /// bind names them.
    pub(crate) fn claims_mouse(&self, button: MouseButton) -> bool {
        self.active
            && match button {
                MouseButton::Left | MouseButton::Right => self.orbit,
                other => self.mouse.contains(&other),
            }
    }

    /// The binding a claimed `key` runs instead, and whether its release runs it again.
    pub(crate) fn remap(&self, key: KeyCode) -> Option<(&str, bool)> {
        if !self.active {
            return None;
        }
        self.remap
            .iter()
            .find(|(k, ..)| *k == key)
            .map(|(_, command, again)| (command.as_str(), *again))
    }

    /// Bring the cursor up on its own: the next movement key puts it away.
    fn summon_cursor(&mut self) {
        if self.orbit {
            self.orbit = false;
            self.auto_cursor = true;
        }
    }

    /// Fill the claims from the binds.
    fn claim(&mut self, config: &SoulsConfig) {
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

/// The keys of the `Shift+` combat binds.
fn shifted_keys(config: &SoulsConfig) -> Vec<SoulsKey> {
    let mut keys = Vec::new();
    for bind in config.binds.combat() {
        if bind.shift && !keys.contains(&bind.key) {
            keys.push(bind.key);
        }
    }
    keys
}

/// Turn the layout on and off with the mode, the switch and the world; the cursor key's tap and
/// the panel keys. Runs before the UI's input feed, which hides the cursor by it.
#[allow(clippy::too_many_arguments)]
pub(super) fn claim_inputs(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    capture: Res<UiKeyboardCapture>,
    bindings: Res<BindingsState>,
    view: Res<CombatModeView>,
    config: Res<SoulsConfig>,
    client: Option<Res<State<ClientState>>>,
    mut er: ResMut<ErControls>,
) {
    let in_world = client.is_some_and(|state| *state.get() == ClientState::InWorld);
    let souls = souls_on(&view) && in_world;
    let active = souls && config.features.er_controls;
    if !active {
        let shifted = if souls { shifted_keys(&config) } else { Vec::new() };
        if er.active || er.orbit || er.shifted != shifted {
            *er = ErControls {
                shifted,
                ..ErControls::default()
            };
        }
        return;
    }
    if !er.active {
        // On: the orbit first, as ER plays.
        er.active = true;
        er.orbit = true;
        er.auto_cursor = false;
        er.cursor_tap = false;
    }
    if config.is_changed() || er.keys.is_empty() {
        let config = &*config;
        er.claim(config);
        er.shifted = shifted_keys(config);
    }
    er.free_camera = config.features.free_camera;
    if capture.typing {
        er.cursor_tap = false;
        return;
    }

    // The cursor key: a tap alone toggles the cursor.
    if let SoulsKey::Key(cursor) = config.binds.cursor {
        if keys.just_pressed(cursor) {
            er.cursor_tap = true;
        } else if keys.pressed(cursor)
            && (keys.get_just_pressed().any(|&key| key != cursor)
                || mouse.get_just_pressed().next().is_some())
        {
            er.cursor_tap = false;
        }
        if keys.just_released(cursor) && std::mem::take(&mut er.cursor_tap) {
            er.orbit = !er.orbit;
            er.auto_cursor = false;
            return;
        }
    }
    // A panel key brings the cursor up; then a movement key puts it away.
    let panel = config
        .controls
        .cursor_keys
        .iter()
        .any(|&key| keys.just_pressed(key) && !capture.consumed.contains(&key));
    if panel {
        er.summon_cursor();
    } else if er.auto_cursor && !er.orbit {
        let moved = [
            Input::MoveForward,
            Input::MoveBackward,
            Input::StrafeLeft,
            Input::StrafeRight,
            Input::TurnLeft,
            Input::TurnRight,
        ]
        .into_iter()
        .any(|input| bindings.just_pressed(input));
        if moved {
            er.orbit = true;
            er.auto_cursor = false;
        }
    }
}

/// The things the interact key considers.
type Things<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Guid, &'static ObjectStore, &'static GlobalTransform),
    Without<SelfPlayer>,
>;

/// What a press of interact does to `store`: `None` when it is nothing to use, else whether it
/// opens a window (the cursor comes up for it).
fn usable(relations: &RelationWorld, guid: u64, store: &ObjectStore) -> Option<bool> {
    let fields = &store.0;
    match fields.object_type()? {
        ObjectType::GameObject => {
            let kind = fields.gameobject_type_id();
            let can_use = fields.gameobject_flags() & GO_FLAG_NO_INTERACT == 0
                && USABLE_GO_TYPES.contains(&kind);
            can_use.then_some(WINDOW_GO_TYPES.contains(&kind))
        }
        ObjectType::Unit => {
            if fields.unit_is_dead() {
                (fields.unit_dynamic_flags() & DYNFLAG_LOOTABLE != 0).then_some(true)
            } else {
                (fields.unit_npc_flags() != 0 && !attackable(relations, guid, store))
                    .then_some(true)
            }
        }
        ObjectType::Corpse => fields.corpse_lootable().then_some(true),
        _ => None,
    }
}

/// The interact key: use the selection in reach, else the best usable thing in front.
#[allow(clippy::too_many_arguments)]
pub(super) fn interact_key(
    keys: Res<ButtonInput<KeyCode>>,
    capture: Res<UiKeyboardCapture>,
    config: Res<SoulsConfig>,
    selection: Res<Selection>,
    player: Res<Player>,
    cams: Query<&FlyCam>,
    relations: RelationWorld,
    things: Things,
    mut er: ResMut<ErControls>,
    mut interact: MessageWriter<Interact>,
) {
    let SoulsKey::Key(key) = config.binds.interact else {
        return;
    };
    let super_down = keys.pressed(KeyCode::SuperLeft) || keys.pressed(KeyCode::SuperRight);
    if !er.active
        || !keys.just_pressed(key)
        || capture.typing
        || capture.consumed.contains(&key)
        || super_down
    {
        return;
    }
    let facing = match cams.single() {
        Ok(cam) if er.free_camera => cam.yaw(),
        _ => player.facing(),
    };
    let front = INTERACT_FRONT_DEGREES.to_radians();
    // Reach, and the pick's cost: distance plus the angle off the facing.
    let cost = |at: Vec3| {
        let to = at - player.pos;
        let flat = Vec2::new(to.x, to.z).length();
        if flat > INTERACT_YARDS || to.y.abs() > INTERACT_HEIGHT_YARDS {
            return None;
        }
        if flat <= INTERACT_UNDERFOOT_YARDS {
            return Some(flat);
        }
        let off = off_facing(facing, to).abs();
        (off <= front).then_some(flat + off * INTERACT_ANGLE_COST)
    };
    let selected = selection.target.and_then(|entity| things.get(entity).ok()).and_then(
        |(entity, guid, store, at)| {
            let to = at.translation() - player.pos;
            let near = Vec2::new(to.x, to.z).length() <= INTERACT_YARDS
                && to.y.abs() <= INTERACT_HEIGHT_YARDS;
            if !near {
                return None;
            }
            usable(&relations, guid.0, store).map(|window| (entity, window))
        },
    );
    let pick = selected.or_else(|| {
        things
            .iter()
            .filter_map(|(entity, guid, store, at)| {
                let cost = cost(at.translation())?;
                let window = usable(&relations, guid.0, store)?;
                Some((cost, entity, window))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, entity, window)| (entity, window))
    });
    let Some((entity, window)) = pick else {
        debug!("souls: nothing to interact with");
        return;
    };
    interact.write(Interact(entity));
    if window {
        er.summon_cursor();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default layout claims its keys and remaps, takes the side buttons but leaves the
    /// primary two to the orbit, and pairs the Shift binds; off, it claims nothing.
    #[test]
    fn default_layout_claims() {
        let config = SoulsConfig::default();
        let mut er = ErControls::default();
        er.claim(&config);
        er.shifted = shifted_keys(&config);
        assert!(!er.claims_key(KeyCode::Space), "inactive claims nothing");
        er.active = true;
        er.orbit = true;
        for key in [KeyCode::Space, KeyCode::KeyQ, KeyCode::KeyE, KeyCode::KeyF, KeyCode::KeyR] {
            assert!(er.claims_key(key), "{key:?}");
        }
        assert!(!er.claims_key(KeyCode::Digit1), "the hotbar keeps its keys");
        assert!(!er.claims_key(KeyCode::KeyW), "movement keeps its keys");
        assert_eq!(er.remap(KeyCode::KeyF), Some(("JUMP", false)));
        assert_eq!(er.remap(KeyCode::KeyR), Some(("ACTIONBUTTON12", false)));
        assert_eq!(er.remap(KeyCode::AltLeft), Some(("TOGGLERUN", true)));
        assert!(er.claims_mouse(MouseButton::Left) && er.claims_mouse(MouseButton::Middle));
        assert!(er.hides_cursor());
        er.summon_cursor();
        assert!(!er.claims_mouse(MouseButton::Left), "the cursor gives the buttons back");
        assert!(er.claims_mouse(MouseButton::Back));
        assert!(!er.hides_cursor());
        assert!(er.shifted.contains(&SoulsKey::Mouse(MouseButton::Left)));
        assert!(er.shifted.contains(&SoulsKey::Mouse(MouseButton::Right)));
    }
}
