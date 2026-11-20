//! Modified clicks, the 3.3.5a table beside the key bindings: each `<ModifiedClick>` row a
//! `Bindings.xml` declares names an action (`SELFCAST`, `STICKYCAMERA`, `CHATLINK`, …) whose
//! chord, a modifier set and an optional mouse button, the stock code asks about with
//! `IsModifiedClick` and the controls panel edits with `SetModifiedClick`. As in the reference
//! (`CGUIBindings::m_modifiedClicks`, `MODIFIEDCLICK::m_data[4]`) a chord is held per binding set,
//! so `LoadBindings`/`SaveBindings` move the clicks with the keys.
//!
//! Here a set holds only the actions it changes from their row's default, as the app's binding
//! file stores keys as a diff ([`super::super::UiScript::seed_click_set`]).

use std::collections::HashMap;

use mlua::{Lua, Value};

use crate::bindings_xml::ModifiedClick;

use super::super::Model;

/// A chord as `MODIFIEDCLICK::SetBinding` (12340 `0x0055E180`) stores it: the modifier bits
/// `CGUIBindings::AddModifiers` (`0x0055D860`) reads (LSHIFT `0x1`, RSHIFT `0x2`, LCTRL `0x4`,
/// RCTRL `0x8`, LALT `0x10`, RALT `0x20`) and a mouse button by its binding index (1 left, 2
/// right, 3 middle, then `Button<n>`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ClickChord {
    modifiers: u8,
    button: Option<u8>,
}

/// The `AddModifiers` names, longest match first within a family as the reference tests them.
const MODIFIER_NAMES: [(&str, u8); 9] = [
    ("LSHIFT", 0x01),
    ("RSHIFT", 0x02),
    ("SHIFT", 0x03),
    ("LCTRL", 0x04),
    ("RCTRL", 0x08),
    ("CTRL", 0x0C),
    ("LALT", 0x10),
    ("RALT", 0x20),
    ("ALT", 0x30),
];

/// `AddModifiers`: leading modifier names, each optionally followed by `-`, case-insensitively;
/// the bits and the rest of the text.
fn leading_modifiers(mut rest: &str) -> (u8, &str) {
    let mut bits = 0;
    while let Some((len, b)) = MODIFIER_NAMES.iter().find_map(|(name, b)| {
        rest.get(..name.len())
            .filter(|p| p.eq_ignore_ascii_case(name))
            .map(|_| (name.len(), *b))
    }) {
        bits |= b;
        rest = &rest[len..];
        rest = rest.strip_prefix('-').unwrap_or(rest);
    }
    (bits, rest)
}

/// `GetMouseButtonName(IndexToMouseButton(i))` (`0x005EEC20`): the name a click handler is passed.
fn mouse_button_name(index: u8) -> String {
    match index {
        1 => "LeftButton".into(),
        2 => "RightButton".into(),
        3 => "MiddleButton".into(),
        n => format!("Button{n}"),
    }
}

impl ClickChord {
    /// `SetBinding`'s parse: empty or `NONE` clears; else modifiers, then `BUTTON` and the one
    /// character the reference steps over (`binding++` after `SStrToInt`), a digit 1-9 naming the
    /// button and anything else none; any text left over refuses the chord.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        if text.is_empty() || text.eq_ignore_ascii_case("NONE") {
            return Some(Self::default());
        }
        let (modifiers, mut rest) = leading_modifiers(text);
        let mut button = None;
        if rest
            .get(..6)
            .is_some_and(|p| p.eq_ignore_ascii_case("BUTTON"))
        {
            let mut after = rest[6..].chars();
            let c = after.next()?;
            button = c.to_digit(10).filter(|&d| d >= 1).map(|d| d as u8);
            rest = after.as_str();
        }
        rest.is_empty().then_some(Self { modifiers, button })
    }

    /// `MODIFIEDCLICK::GetBinding` (`0x0055E230`): `NONE`, or the `AddMetaPrefix` prefixes
    /// (`0x0055D7A0`: ALT, CTRL, SHIFT, a single side spelled `L`/`R`) and `BUTTON<n>`.
    pub(crate) fn render(self) -> String {
        if self.modifiers == 0 && self.button.is_none() {
            return "NONE".into();
        }
        let mut parts: Vec<String> = Vec::new();
        for (mask, both, left, right) in [
            (0x30, "ALT", "LALT", "RALT"),
            (0x0C, "CTRL", "LCTRL", "RCTRL"),
            (0x03, "SHIFT", "LSHIFT", "RSHIFT"),
        ] {
            let bits = self.modifiers & mask;
            if bits == mask {
                parts.push(both.into());
            } else if bits != 0 && bits & mask.isolate_lowest_one() != 0 {
                parts.push(left.into());
            } else if bits != 0 {
                parts.push(right.into());
            }
        }
        if let Some(b) = self.button {
            parts.push(format!("BUTTON{b}"));
        }
        parts.join("-")
    }

    /// `MODIFIEDCLICK::IsActive` (`0x0055D6B0`): any one of the chord's modifiers held (a chord with
    /// modifiers and no button is then active), and a chord's button matches the click being
    /// dispatched or any time outside a click. A chord with neither is never active.
    ///
    /// Deviation: the host tracks shift, ctrl and alt without their side, so `LSHIFT` reads as
    /// either shift key.
    fn active(self, held: (bool, bool, bool), clicking: Option<&str>) -> bool {
        if self.modifiers != 0 {
            let (shift, ctrl, alt) = held;
            let any = (self.modifiers & 0x03 != 0 && shift)
                || (self.modifiers & 0x0C != 0 && ctrl)
                || (self.modifiers & 0x30 != 0 && alt);
            if !any {
                return false;
            }
            if self.button.is_none() {
                return true;
            }
        }
        let Some(button) = self.button else {
            return false;
        };
        clicking.is_none_or(|c| c == mouse_button_name(button))
    }
}

/// One registered row: the declared action and its default chord.
#[derive(Clone, Debug)]
struct Row {
    action: String,
    default: ClickChord,
}

/// The actions' sets: the rows in registration order, then per set the chords that differ from a
/// row's default, keyed by the upper-cased action (the reference's table folds case).
#[derive(Clone, Default)]
pub(crate) struct ClickState {
    rows: Vec<Row>,
    /// The live set, `BINDING_SET_CURRENT`, which every getter and `SetModifiedClick` use.
    live: HashMap<String, ClickChord>,
    /// Stored sets: `[0]` account (set 1), `[1]` character (set 2, if any).
    stored: [Option<HashMap<String, ClickChord>>; 2],
}

impl ClickState {
    fn row(&self, action: &str) -> Option<&Row> {
        self.rows
            .iter()
            .find(|r| r.action.eq_ignore_ascii_case(action))
    }

    /// The action's live chord; `None` for an action no file declared.
    fn chord(&self, action: &str) -> Option<ClickChord> {
        let row = self.row(action)?;
        Some(
            self.live
                .get(&action.to_ascii_uppercase())
                .copied()
                .unwrap_or(row.default),
        )
    }

    /// `LoadModifiedClick` (`0x00562DD0`): a new action joins with its default; an action already
    /// declared, in this file or an earlier one, is refused. An unparsable default leaves the row
    /// at `NONE`, as the refused `SetBinding` leaves the zeroed slot.
    pub(super) fn register(&mut self, rows: &[ModifiedClick]) {
        for row in rows {
            if self.row(&row.action).is_some() {
                continue;
            }
            let default = row
                .default
                .as_deref()
                .and_then(ClickChord::parse)
                .unwrap_or_default();
            self.rows.push(Row {
                action: row.action.clone(),
                default,
            });
        }
    }

    /// The clicks' half of `LoadBindings(set)`, with the keys' fallbacks: the character set falls
    /// back to the account set, either to the defaults.
    pub(super) fn load(&mut self, set: u32) {
        self.live = match set {
            0 => HashMap::new(),
            1 => self.stored[0].clone().unwrap_or_default(),
            2 => self.stored[1]
                .clone()
                .or_else(|| self.stored[0].clone())
                .unwrap_or_default(),
            _ => return,
        };
    }

    /// The clicks' half of `SaveBindings(which)` (`0x005640C0`): store the live set; saving the
    /// account set drops the character set.
    pub(super) fn save(&mut self, which: u32) {
        if which != 1 && which != 2 {
            return;
        }
        self.stored[(which - 1) as usize] = Some(self.live.clone());
        if which == 1 {
            self.stored[1] = None;
        }
    }

    /// Seed stored set 1 or 2 from the app's file; a chord that does not parse is dropped.
    pub(super) fn seed(&mut self, set: u32, overrides: Option<Vec<(String, String)>>) {
        let overrides = overrides.map(|list| {
            list.into_iter()
                .filter_map(|(action, chord)| {
                    Some((action.to_ascii_uppercase(), ClickChord::parse(&chord)?))
                })
                .collect()
        });
        match set {
            1 => self.stored[0] = overrides,
            2 => self.stored[1] = overrides,
            _ => {}
        }
    }

    /// The live set's changes for the app's file, `(action, chord)` sorted by action: every action
    /// whose chord differs from its row's default, and any undeclared action a file seeded (an
    /// addon that declares it may load later).
    pub(super) fn overrides(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = self
            .live
            .iter()
            .filter(|(action, chord)| self.row(action).is_none_or(|r| r.default != **chord))
            .map(|(action, chord)| (action.clone(), chord.render()))
            .collect();
        out.sort();
        out
    }
}

/// The mouse button of the click being dispatched, the reference's
/// `CSimpleTop::m_mouseButtonName`, which `CSimpleButton::OnClick` (`0x0096FD70`) and the frame's
/// `OnMouseDown`/`OnMouseUp` legs set around their scripts and restore after.
pub(crate) fn with_click_button<R>(lua: &Lua, button: &str, f: impl FnOnce() -> R) -> R {
    let saved = lua
        .app_data_mut::<Model>()
        .expect("model app_data")
        .click_button
        .replace(button.to_owned());
    let out = f();
    lua.app_data_mut::<Model>()
        .expect("model app_data")
        .click_button = saved;
    out
}

/// `CGUIBindings::IsModifiedClick` (`0x0055F940`). No action: any modifier held, or any action's
/// chord active. An action that opens with modifier names asks for those modifiers, every family
/// named held. Else the declared action's chord; an undeclared one is false.
fn is_modified_click(model: &Model, action: Option<&str>) -> bool {
    let held = model.modifiers;
    let clicking = model.click_button.as_deref();
    let clicks = &model.keybinds.clicks;
    let Some(action) = action else {
        return held.0
            || held.1
            || held.2
            || clicks.rows.iter().any(|r| {
                clicks
                    .chord(&r.action)
                    .is_some_and(|c| c.active(held, clicking))
            });
    };
    let (bits, _) = leading_modifiers(action);
    if bits == 0 {
        return clicks
            .chord(action)
            .is_some_and(|c| c.active(held, clicking));
    }
    (bits & 0x03 == 0 || held.0) && (bits & 0x0C == 0 || held.1) && (bits & 0x30 == 0 || held.2)
}

/// The 3.3.5a modified-click globals (`UIBindings_Scripts.cpp`). The 1.12 client has none.
pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    // `0x0055DC60`.
    g.set(
        "GetNumModifiedClickActions",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(model.keybinds.clicks.rows.len())
        })?,
    )?;
    // `0x0055EA70`: `lua_isnumber` else the usage; out of range pushes a null string, nil.
    g.set(
        "GetModifiedClickAction",
        lua.create_function(|lua, index: Value| {
            let Some(index) = super::super::binding_abi::is_number(lua, index) else {
                return Err(mlua::Error::RuntimeError(
                    "Usage: GetModifiedClickAction(index)".into(),
                ));
            };
            let i = super::super::binding_abi::ftol(index).wrapping_sub(1);
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(usize::try_from(i)
                .ok()
                .and_then(|i| model.keybinds.clicks.rows.get(i))
                .map(|r| r.action.clone()))
        })?,
    )?;
    // `0x0055FC20`: the live chord, nil for an undeclared action.
    g.set(
        "GetModifiedClick",
        lua.create_function(|lua, action: Value| {
            let action = super::super::binding_abi::string_arg(
                lua,
                action,
                "Usage: GetModifiedClick(\"action\")",
            )?;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(model.keybinds.clicks.chord(&action).map(ClickChord::render))
        })?,
    )?;
    // `0x0055FB90`: the live set only; nothing is signalled and nothing persists until
    // `SaveBindings`. A missing binding is `NONE`.
    g.set(
        "SetModifiedClick",
        lua.create_function(|lua, (action, binding): (Value, Option<Value>)| {
            let action = super::super::binding_abi::string_arg(
                lua,
                action,
                "Usage: SetModifiedClick(\"action\", \"binding\")",
            )?;
            let binding = super::super::binding_abi::tolstring_arg(lua, binding);
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            let clicks = &mut model.keybinds.clicks;
            let chord = binding
                .as_deref()
                .map_or(Some(ClickChord::default()), ClickChord::parse);
            match (clicks.row(&action).is_some(), chord) {
                (true, Some(chord)) => {
                    clicks.live.insert(action.to_ascii_uppercase(), chord);
                    Ok(())
                }
                _ => Err(mlua::Error::RuntimeError(format!(
                    "SetModifiedClick(): Unknown action ({action}) or binding ({})",
                    binding.as_deref().unwrap_or("(null)")
                ))),
            }
        })?,
    )?;
    // `0x0055FCC0`: 1 or nil.
    g.set(
        "IsModifiedClick",
        lua.create_function(|lua, action: Option<Value>| {
            let action = super::super::binding_abi::tolstring_arg(lua, action);
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(super::super::binding_abi::flag(is_modified_click(
                &model,
                action.as_deref(),
            )))
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ClickChord;

    /// The stock rows' spellings parse and render back as the reference's `GetBinding` writes them.
    #[test]
    fn chords_parse_and_render_in_the_reference_spelling() {
        for (text, rendered) in [
            ("ALT", "ALT"),
            ("NONE", "NONE"),
            ("", "NONE"),
            ("SHIFT-BUTTON1", "SHIFT-BUTTON1"),
            ("ctrl-button2", "CTRL-BUTTON2"),
            ("SHIFT-CTRL", "CTRL-SHIFT"),
            ("LSHIFT", "LSHIFT"),
            ("RALT-BUTTON3", "RALT-BUTTON3"),
        ] {
            let chord = ClickChord::parse(text).unwrap_or_else(|| panic!("{text} parses"));
            assert_eq!(chord.render(), rendered, "{text}");
        }
        for refused in ["SHIFT-Q", "BUTTON", "META", "SHIFT-BUTTON12"] {
            assert_eq!(ClickChord::parse(refused), None, "{refused} is refused");
        }
    }
}
