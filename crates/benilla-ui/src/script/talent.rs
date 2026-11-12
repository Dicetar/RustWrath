//! The talent bindings: the app pushes the class's pages and talents, resolved from `Talent.dbc`
//! and `Spell.dbc` ([`UiScript::set_talents`]), and `LearnTalent(tab, index)` sends
//! `CMSG_LEARN_TALENT` inside its call ([`TalentCall`]). Grid seats, ranks, prerequisites and
//! availability are the app's.
//!
//! The tuples are what `Blizzard_TalentUI.lua` reads (`:154`, `:180`): `GetTalentTabInfo(i)` gives
//! name, texture, pointsSpent, fileName; `GetTalentInfo(tab, i)` name, icon, tier, column, rank,
//! maxRank, isExceptional, meetsPrereq. Tiers and columns are 1-based, as the stock frame indexes
//! `TALENT_BRANCH_ARRAY[tier][column]`. The respec pair answers a class trainer's
//! `CONFIRM_TALENT_WIPE` and reads no snapshot.
//!
//! `GameTooltip:SetTalent` (`0x535170`) builds an exceptional talent with the spell builder
//! (`0x52e610`) and any other with the talent builder (`0x52b0a0`), which keeps the spell's name
//! and description and drops the rest of its body. The talent lines are the talent builder's: the
//! rank line (`TOOLTIP_TALENT_RANK`, `0x854a2c`), the red requirement lines after it, the "Next
//! rank:" block (`TOOLTIP_TALENT_NEXT_RANK`, `0x854a10`, white, over the next rank's gold
//! description) and "Click to learn" (`TOOLTIP_TALENT_LEARN`, `0x8549f8`) in green on a learnable
//! rank. The spell builder writes the same lines around the full body (`0x52f735`); its next-rank
//! pass, a second full body, never runs, as no 1.12 exceptional talent has a second rank.

use mlua::{Lua, MultiValue, Table, Value};

use super::binding_abi::{self, flag};
use super::tooltip_spell::{spell_view_of, TalentLines};
use super::Model;

/// One talent page, a `TalentTab.dbc` row: `GetTalentTabInfo`'s source.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TalentTabView {
    pub name: String,
    /// The `Interface\TalentFrame\<base>-` art base, the tuple's `fileName`.
    pub background: String,
    pub points_spent: u32,
    /// The tab's `SpellIcon.dbc` path (`TalentTab.dbc` `SpellIconID`, `0x4f30d0`-`0x4f30f1`), the
    /// tuple's `texture`; `None` pushes nil (`0x4f30f8`).
    pub texture: Option<String>,
}

/// One prerequisite edge, a `GetTalentPrereqs` triplet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TalentPrereqView {
    pub tier: u32,
    pub column: u32,
    /// The prerequisite is learned to its required rank.
    pub learnable: bool,
}

/// One talent button: `GetTalentInfo`'s source and the tooltip's context.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TalentView {
    /// The `Talent.dbc` id, which `CMSG_LEARN_TALENT` carries (`0x4f37c7`).
    pub talent_id: u32,
    pub name: String,
    pub texture: Option<String>,
    pub tier: u32,
    pub column: u32,
    /// Current rank, 0 when unlearned.
    pub rank: u32,
    pub max_rank: u32,
    /// `Talent.dbc` flags bit 0 (`TalentRec+0x4c`), the tuple's `isExceptional`; the stock frame
    /// ignores it.
    pub exceptional: bool,
    /// The required spell is known; prerequisite talents live in the triplets.
    pub meets_prereq: bool,
    pub prereqs: Vec<TalentPrereqView>,
    /// The spell id of rank `max(1, rank)`, the tooltip's spell part.
    pub display_spell: u32,
    /// The next rank's spell id when `0 < rank < max_rank`, else 0.
    pub next_spell: u32,
    /// The red requirement lines of an unlearned talent, in `0x52b390`'s order; their words are
    /// read as the tooltip builds ([`TalentReq`]).
    pub reqs: Vec<TalentReq>,
    /// A higher rank is learnable now, every requirement met, a point to spend and a rank below
    /// the maximum: the green `TOOLTIP_TALENT_LEARN` line (`0x52b31b`, `0x52f735`).
    pub learnable: bool,
}

/// One red requirement line, its template a `_G` read at the tooltip's build (`0x703bf0`): an
/// addon's rewording shows at the next hover, and a key it lacks renders no line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TalentReq {
    /// `TOOLTIP_TALENT_PREREQ`, its `_P1` twin by the rank (`0x52b4a5`, the ordinal in `edx`), of
    /// the 1-based rank needed and the prerequisite's name.
    Prereq { rank: u32, name: String },
    /// `TOOLTIP_TALENT_TIER_POINTS` (`0x52b566`) of the points the tier needs and the tab's name.
    Tier { points: u32, tab: String },
    /// `ITEM_REQ_SKILL` (`0x52b60b`) of the required spell's name.
    Spell { name: String },
}

impl TalentReq {
    /// The line as `0x52b390` writes it now, `None` for a missing or empty template.
    fn line(&self, lua: &Lua) -> Option<String> {
        use crate::strings::{fill, global, plural, Arg};
        let get = |key: &str| global(lua, key);
        Some(match self {
            Self::Prereq { rank, name } => fill(
                &plural("TOOLTIP_TALENT_PREREQ", Some(*rank), &get)?,
                &[Arg::D(i64::from(*rank)), Arg::S(name)],
            ),
            Self::Tier { points, tab } => fill(
                &get("TOOLTIP_TALENT_TIER_POINTS")?,
                &[Arg::D(i64::from(*points)), Arg::S(tab)],
            ),
            Self::Spell { name } => fill(&get("ITEM_REQ_SKILL")?, &[Arg::S(name)]),
        })
    }
}

/// The pushed snapshot: the player's own pages and unspent points.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TalentUiState {
    pub tabs: Vec<TalentTabView>,
    /// `talents[t]` holds tab `t+1`'s talents, in the app's enumeration order.
    pub talents: Vec<Vec<TalentView>>,
    /// `UnitCharacterPoints("player")`: unspent talent points, free primary professions.
    pub points: (u32, u32),
}

/// One 3.3.5a player talent group (`SMSG_TALENTS_INFO` spec slot 0 or 1), with its computed
/// tab/talent views, unspent points, and 6 `GlyphProperties.dbc` socket IDs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WrathTalentGroupView {
    pub tabs: Vec<TalentTabView>,
    pub talents: Vec<Vec<TalentView>>,
    pub unspent_points: u32,
    pub glyphs: [u16; 6],
}

/// The 3.3.5a dual-spec, pet-talent, and glyph snapshot pushed alongside [`TalentUiState`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WrathTalentUiState {
    /// 1-based active talent group (`1` or `2`).
    pub active_group: u8,
    pub groups: Vec<WrathTalentGroupView>,
    pub pet_unspent_points: u32,
    pub pet_talents: Vec<(u32, u8)>,
    pub pet_tabs: Vec<TalentTabView>,
    pub pet_talent_views: Vec<Vec<TalentView>>,
}

impl super::UiScript {
    /// Push the whole talent snapshot.
    pub fn set_talents(&mut self, state: TalentUiState) {
        self.model_mut().talents = state;
    }

    /// Push the 3.3.5a dual-spec talent groups, pet unspent points, and glyph socket IDs.
    pub fn set_wrath_talents(&mut self, state: WrathTalentUiState) {
        self.model_mut().wrath_talents = state;
    }

    /// The talent verbs a harness's VM kept ([`Self::keep_calls`]), in call order.
    pub fn take_talent_calls(&mut self) -> Vec<TalentCall> {
        self.take_calls_where(|call| match call {
            super::ScriptCall::Talent(call) => Some(*call),
            _ => None,
        })
    }

    /// Push whether a trainer's respec question is live and in range, which
    /// `CheckTalentMasterDist()` answers; false for none pending or walked away.
    pub fn set_talent_master_pending(&mut self, pending: bool) {
        let mut model = self.model_mut();
        if model.talent_master_pending != pending {
            model.talent_master_pending = pending;
        }
    }
}

/// A talent verb the host applies inside its call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TalentCall {
    /// `LearnTalent(tab, index)` (`0x4f36a0`) on a talent below its maximum (`0x4f378f`):
    /// `CMSG_LEARN_TALENT` (`0x251`) with the talent id and its rank count, the 0-based next rank
    /// (`0x4f37c7`, `0x4f37d1`).
    Learn { talent_id: u32, rank: u32 },
    /// `ConfirmTalentWipe()` (`0x48dc40`): the respec dialog `0x5df980` with a zero guid, which
    /// answers the trainer it latched.
    ConfirmWipe,
}

fn is_truthy_arg(arg: Option<&Value>) -> bool {
    matches!(arg, Some(Value::Boolean(true)) | Some(Value::Integer(1..)))
        || matches!(arg, Some(Value::Number(n)) if *n >= 1.0)
}

fn wrath_group_index(model: &Model, arg: Option<&Value>) -> usize {
    let requested = match arg {
        Some(Value::Integer(n)) if *n >= 1 => *n as usize,
        Some(Value::Number(n)) if *n >= 1.0 => *n as usize,
        _ => usize::from(model.wrath_talents.active_group.max(1)),
    };
    requested.saturating_sub(1)
}

fn talent_at_scope<'a>(
    model: &'a Model,
    tab: usize,
    index: usize,
    is_pet: bool,
    group_arg: Option<&Value>,
) -> Option<&'a TalentView> {
    if model.target == super::UiTarget::Wrath335 {
        if is_pet {
            return model
                .wrath_talents
                .pet_talent_views
                .get(tab.checked_sub(1)?)
                .and_then(|t| t.get(index.checked_sub(1)?));
        }
        let group_idx = wrath_group_index(model, group_arg);
        if let Some(group) = model.wrath_talents.groups.get(group_idx) {
            return group
                .talents
                .get(tab.checked_sub(1)?)
                .and_then(|t| t.get(index.checked_sub(1)?));
        }
    }
    talent_at(model, tab, index)
}

/// One talent by its 1-based (tab, index).
fn talent_at(model: &Model, tab: usize, index: usize) -> Option<&TalentView> {
    model
        .talents
        .talents
        .get(tab.checked_sub(1)?)
        .and_then(|t| t.get(index.checked_sub(1)?))
}

/// Register the talent globals.
pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();

    g.set(
        "GetNumTalentTabs",
        lua.create_function(|lua, args: MultiValue| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            if model.target == super::UiTarget::Wrath335 {
                let is_pet = is_truthy_arg(args.get(1));
                if is_pet {
                    if !model.wrath_talents.pet_tabs.is_empty() {
                        return Ok(model.wrath_talents.pet_tabs.len() as i64);
                    }
                    if !model.wrath_talents.pet_talents.is_empty()
                        || model.wrath_talents.pet_unspent_points > 0
                    {
                        return Ok(1i64);
                    }
                    return Ok(0i64);
                }
                let group_idx = wrath_group_index(&model, None);
                if let Some(group) = model.wrath_talents.groups.get(group_idx) {
                    if !group.tabs.is_empty() {
                        return Ok(group.tabs.len() as i64);
                    }
                }
            }
            Ok(model.talents.tabs.len() as i64)
        })?,
    )?;

    // Name, the tab's `SpellIcon.dbc` path (nil for an id the table lacks), points spent and the
    // background (`0x4f30cb`-`0x4f3121`). Out of range answers nil, nil, 0, nil (`0x4f3132`).
    // In Wrath 3.3.5a (`TalentFrameBase.lua`), a 5th return `previewPointsSpent` (0) is read.
    g.set(
        "GetTalentTabInfo",
        lua.create_function(|lua, args: MultiValue| {
            // `is-number` else `Usage: GetTalentTabInfo(index)` (`0x84cdd4`, `0x4f3059`).
            let i = args.front().cloned().unwrap_or(Value::Nil);
            let i = binding_abi::index_arg(lua, i, "Usage: GetTalentTabInfo(index)")?;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let wrath = model.target == super::UiTarget::Wrath335;
            let is_pet = wrath && is_truthy_arg(args.get(2));
            if is_pet && model.wrath_talents.pet_tabs.is_empty() && i == 1 {
                if !model.wrath_talents.pet_talents.is_empty()
                    || model.wrath_talents.pet_unspent_points > 0
                {
                    let spent: i64 = model
                        .wrath_talents
                        .pet_talents
                        .iter()
                        .map(|&(_, max_rank_idx)| i64::from(max_rank_idx) + 1)
                        .sum();
                    return Ok(MultiValue::from_vec(vec![
                        Value::String(lua.create_string("Pet")?),
                        Value::Nil,
                        Value::Integer(spent),
                        Value::String(lua.create_string("HunterPetFerocity")?),
                        Value::Integer(0),
                    ]));
                }
            }
            let tabs = if is_pet {
                &model.wrath_talents.pet_tabs
            } else if wrath {
                let group_idx = wrath_group_index(&model, args.get(3));
                model
                    .wrath_talents
                    .groups
                    .get(group_idx)
                    .map_or(&model.talents.tabs, |g| &g.tabs)
            } else {
                &model.talents.tabs
            };
            let Some(tab) = i.checked_sub(1).and_then(|n| tabs.get(n)) else {
                if wrath {
                    return Ok(super::binding_abi::tuple(&[None, None, Some(0), None, Some(0)]));
                }
                return Ok(super::binding_abi::tuple(&[None, None, Some(0), None]));
            };
            let mut out = vec![
                Value::String(lua.create_string(&tab.name)?),
                match &tab.texture {
                    Some(texture) => Value::String(lua.create_string(texture)?),
                    None => Value::Nil,
                },
                Value::Integer(i64::from(tab.points_spent)),
                Value::String(lua.create_string(&tab.background)?),
            ];
            if wrath {
                out.push(Value::Integer(0));
            }
            Ok(MultiValue::from_vec(out))
        })?,
    )?;

    g.set(
        "GetNumTalents",
        lua.create_function(|lua, args: MultiValue| {
            // `is-number` else `Usage: GetNumTalents(tabIndex)` (`0x84cdf4`, `0x4f3177`).
            let tab = args.front().cloned().unwrap_or(Value::Nil);
            let tab = binding_abi::index_arg(lua, tab, "Usage: GetNumTalents(tabIndex)")?;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let wrath = model.target == super::UiTarget::Wrath335;
            if wrath && is_truthy_arg(args.get(2)) {
                if !model.wrath_talents.pet_talent_views.is_empty() {
                    let n = tab
                        .checked_sub(1)
                        .and_then(|t| model.wrath_talents.pet_talent_views.get(t))
                        .map_or(0, Vec::len);
                    return Ok(n as i64);
                }
                return Ok(if tab == 1 {
                    model.wrath_talents.pet_talents.len() as i64
                } else {
                    0
                });
            }
            if wrath {
                let group_idx = wrath_group_index(&model, None);
                if let Some(group) = model.wrath_talents.groups.get(group_idx) {
                    if let Some(list) = tab.checked_sub(1).and_then(|t| group.talents.get(t)) {
                        return Ok(list.len() as i64);
                    }
                }
            }
            let n = tab
                .checked_sub(1)
                .and_then(|t| model.talents.talents.get(t))
                .map_or(0, Vec::len);
            Ok(n as i64)
        })?,
    )?;

    g.set(
        "GetTalentInfo",
        lua.create_function(|lua, args: MultiValue| {
            // Both `is-number` else `Usage: GetTalentInfo(tabIndex, talentIndex)` (`0x84ce14`,
            // `0x4f34ae`).
            let tab = args.front().cloned().unwrap_or(Value::Nil);
            let i = args.get(1).cloned().unwrap_or(Value::Nil);
            let tab =
                binding_abi::index_arg(lua, tab, "Usage: GetTalentInfo(tabIndex, talentIndex)")?;
            let i = binding_abi::index_arg(lua, i, "Usage: GetTalentInfo(tabIndex, talentIndex)")?;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let wrath = model.target == super::UiTarget::Wrath335;
            let is_pet = wrath && is_truthy_arg(args.get(3));
            let Some(t) = talent_at_scope(&model, tab, i, is_pet, args.get(4)) else {
                // `0x4f3454`: nil, nil, 1, 1, 0, 0, nil, nil.
                if wrath {
                    return Ok(super::binding_abi::tuple(&[
                        None,
                        None,
                        Some(1),
                        Some(1),
                        Some(0),
                        Some(0),
                        None,
                        None,
                        Some(0),
                        None,
                    ]));
                }
                return Ok(super::binding_abi::tuple(&[
                    None,
                    None,
                    Some(1),
                    Some(1),
                    Some(0),
                    Some(0),
                    None,
                    None,
                ]));
            };
            let texture = match &t.texture {
                Some(tex) => Value::String(lua.create_string(tex)?),
                None => Value::Nil,
            };
            let mut out = vec![
                Value::String(lua.create_string(&t.name)?),
                texture,
                Value::Integer(i64::from(t.tier)),
                Value::Integer(i64::from(t.column)),
                Value::Integer(i64::from(t.rank)),
                Value::Integer(i64::from(t.max_rank)),
                flag(t.exceptional),
                flag(t.meets_prereq),
            ];
            if wrath {
                out.push(Value::Integer(i64::from(t.rank)));
                out.push(flag(t.meets_prereq));
            }
            Ok(MultiValue::from_vec(out))
        })?,
    )?;

    // Flat (tier, column, isLearnable) triplets, as `Blizzard_TalentUI.lua:385` walks them.
    g.set(
        "GetTalentPrereqs",
        lua.create_function(|lua, args: MultiValue| {
            // `is-number` on args 1 and 2 else `Usage: GetTalentPrereqs(tabIndex, talentIndex)`
            // (`0x84ce40`, `0x4f3686`).
            let tab = args.front().cloned().unwrap_or(Value::Nil);
            let i = args.get(1).cloned().unwrap_or(Value::Nil);
            let tab =
                binding_abi::index_arg(lua, tab, "Usage: GetTalentPrereqs(tabIndex, talentIndex)")?;
            let i =
                binding_abi::index_arg(lua, i, "Usage: GetTalentPrereqs(tabIndex, talentIndex)")?;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let is_pet =
                model.target == super::UiTarget::Wrath335 && is_truthy_arg(args.get(3));
            let mut out = Vec::new();
            if let Some(t) = talent_at_scope(&model, tab, i, is_pet, args.get(4)) {
                for p in &t.prereqs {
                    out.push(Value::Integer(i64::from(p.tier)));
                    out.push(Value::Integer(i64::from(p.column)));
                    out.push(flag(p.learnable));
                }
            }
            Ok(MultiValue::from_vec(out))
        })?,
    )?;

    // LearnTalent(tab, index [, isPet]): the talent's rank is its known ranks, and one at its
    // maximum sends nothing (`0x4f378f`), the client's only gate; points and prerequisites are the
    // server's (vmangos `Player::LearnTalent`).
    g.set(
        "LearnTalent",
        lua.create_function(|lua, args: MultiValue| {
            // `is-number` on args 1 and 2 else `Usage: LearnTalent(tabIndex, talentIndex)`
            // (`0x84ce70`, `0x4f380c`).
            let tab = args.front().cloned().unwrap_or(Value::Nil);
            let i = args.get(1).cloned().unwrap_or(Value::Nil);
            let tab =
                binding_abi::index_arg(lua, tab, "Usage: LearnTalent(tabIndex, talentIndex)")?;
            let i = binding_abi::index_arg(lua, i, "Usage: LearnTalent(tabIndex, talentIndex)")?;
            let learn = {
                let model = lua.app_data_ref::<Model>().expect("model app_data");
                let is_pet =
                    model.target == super::UiTarget::Wrath335 && is_truthy_arg(args.get(2));
                talent_at_scope(&model, tab, i, is_pet, None)
                    .filter(|t| t.rank < t.max_rank)
                    .map(|t| TalentCall::Learn {
                        talent_id: t.talent_id,
                        rank: t.rank,
                    })
            };
            match learn {
                Some(call) => super::calls::submit(lua, super::ScriptCall::Talent(call)),
                None => Ok(()),
            }
        })?,
    )?;

    // ConfirmTalentWipe(): the `CONFIRM_TALENT_WIPE` Accept, the one call that unlearns talents. No
    // argument: the reference answers the trainer guid it latched (`0xc4d7a0`), which the app holds.
    g.set(
        "ConfirmTalentWipe",
        lua.create_function(|lua, ()| {
            super::calls::submit(lua, super::ScriptCall::Talent(TalentCall::ConfirmWipe))
        })?,
    )?;

    // CheckTalentMasterDist(): polled by that dialog's OnUpdate, nil hides it. The reference
    // re-runs its interact-range test on the latched trainer (`0x5df980`, `d² <= [0xc4c28c]`);
    // no packet either way.
    g.set(
        "CheckTalentMasterDist",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(flag(model.talent_master_pending))
        })?,
    )?;

    // Every unit token answers the player's own pair: `PLAYER_CHARACTER_POINTS1/2` are private
    // fields only we receive.
    g.set(
        "UnitCharacterPoints",
        lua.create_function(|lua, _unit: String| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let (cp1, cp2) = model.talents.points;
            Ok((i64::from(cp1), i64::from(cp2)))
        })?,
    )?;

    if lua.app_data_ref::<Model>().expect("model app_data").target == super::UiTarget::Wrath335 {
        install_wrath(lua, &g)?;
    }

    Ok(())
}

/// Wrath 3.3.5a talent spec group, preview points, and glyph socket bindings consumed by
/// `Blizzard_TalentUI` and `Blizzard_GlyphUI`.
fn install_wrath(lua: &Lua, g: &Table) -> mlua::Result<()> {
    g.set(
        "GetNumTalentGroups",
        lua.create_function(|lua, args: MultiValue| {
            let is_pet =
                matches!(args.get(1), Some(Value::Boolean(true)) | Some(Value::Integer(1..)));
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            if is_pet {
                let has_pet = model.wrath_talents.pet_unspent_points > 0
                    || !model.wrath_talents.pet_talents.is_empty()
                    || !model.wrath_talents.pet_tabs.is_empty();
                return Ok(if has_pet { 1i64 } else { 0i64 });
            }
            Ok(model.wrath_talents.groups.len().max(1) as i64)
        })?,
    )?;
    g.set(
        "GetActiveTalentGroup",
        lua.create_function(|lua, args: MultiValue| {
            let is_pet =
                matches!(args.get(1), Some(Value::Boolean(true)) | Some(Value::Integer(1..)));
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            if is_pet {
                let has_pet = model.wrath_talents.pet_unspent_points > 0
                    || !model.wrath_talents.pet_talents.is_empty()
                    || !model.wrath_talents.pet_tabs.is_empty();
                return Ok(if has_pet { 1i64 } else { 0i64 });
            }
            Ok(i64::from(model.wrath_talents.active_group.max(1)))
        })?,
    )?;
    g.set(
        "SetActiveTalentGroup",
        lua.create_function(|_, _: MultiValue| Ok(()))?,
    )?;
    g.set(
        "GetUnspentTalentPoints",
        lua.create_function(|lua, args: MultiValue| {
            let is_inspect =
                matches!(args.front(), Some(Value::Boolean(true)) | Some(Value::Integer(1..)));
            let is_pet =
                matches!(args.get(1), Some(Value::Boolean(true)) | Some(Value::Integer(1..)));
            if is_inspect {
                return Ok(0i64);
            }
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            if is_pet {
                return Ok(i64::from(model.wrath_talents.pet_unspent_points));
            }
            let group_idx = wrath_group_index(&model, args.get(2));
            if let Some(group) = model.wrath_talents.groups.get(group_idx) {
                return Ok(i64::from(group.unspent_points));
            }
            Ok(i64::from(model.talents.points.0))
        })?,
    )?;
    g.set(
        "GetPreviewTalentPointsSpent",
        lua.create_function(|_, _: MultiValue| Ok(0i64))?,
    )?;
    g.set(
        "GetGroupPreviewTalentPointsSpent",
        lua.create_function(|_, _: MultiValue| Ok(0i64))?,
    )?;
    g.set(
        "AddPreviewTalentPoints",
        lua.create_function(|_, _: MultiValue| Ok(()))?,
    )?;
    g.set(
        "ResetPreviewTalentPoints",
        lua.create_function(|_, _: MultiValue| Ok(()))?,
    )?;
    g.set(
        "ResetGroupPreviewTalentPoints",
        lua.create_function(|_, _: MultiValue| Ok(()))?,
    )?;
    g.set(
        "LearnPreviewTalents",
        lua.create_function(|_, _: MultiValue| Ok(()))?,
    )?;
    g.set(
        "GetTalentLink",
        lua.create_function(|lua, args: MultiValue| {
            let tab = args.front().cloned().unwrap_or(Value::Nil);
            let i = args.get(1).cloned().unwrap_or(Value::Nil);
            let Ok(tab) = binding_abi::index_arg(lua, tab, "Usage: GetTalentLink(tabIndex, talentIndex)") else {
                return Ok(Value::Nil);
            };
            let Ok(i) = binding_abi::index_arg(lua, i, "Usage: GetTalentLink(tabIndex, talentIndex)") else {
                return Ok(Value::Nil);
            };
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let is_pet = is_truthy_arg(args.get(3));
            let Some(t) = talent_at_scope(&model, tab, i, is_pet, args.get(4)) else {
                return Ok(Value::Nil);
            };
            let rank = i64::from(t.rank).saturating_sub(1);
            let link = format!("|cff4e96f7|Htalent:{}:{rank}|h[{}]|h|r", t.talent_id, t.name);
            Ok(Value::String(lua.create_string(&link)?))
        })?,
    )?;

    // `Blizzard_GlyphUI` socket queries (`GlyphFrame.lua`).
    g.set(
        "GetNumGlyphSockets",
        lua.create_function(|_, _: MultiValue| Ok(6i64))?,
    )?;
    g.set(
        "GetGlyphSocketInfo",
        lua.create_function(|lua, args: MultiValue| {
            let socket = match args.front() {
                Some(Value::Integer(n)) => *n,
                Some(Value::Number(n)) => *n as i64,
                _ => 1,
            };
            // Standard 3.3.5a glyph socket layout: 1, 4, 6 are Major (1); 2, 3, 5 are Minor (2).
            let glyph_type = match socket {
                2 | 3 | 5 => 2i64,
                _ => 1i64,
            };
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let group_idx = wrath_group_index(&model, args.get(1));
            if let Some(group) = model.wrath_talents.groups.get(group_idx) {
                if (1..=6).contains(&socket) {
                    let glyph_id = group.glyphs[(socket - 1) as usize];
                    let spell = if glyph_id != 0 {
                        Value::Integer(i64::from(glyph_id))
                    } else {
                        Value::Nil
                    };
                    return Ok(MultiValue::from_vec(vec![
                        Value::Integer(1),
                        Value::Integer(glyph_type),
                        spell,
                        Value::Nil,
                    ]));
                }
            }
            Ok(MultiValue::from_vec(vec![
                Value::Nil,
                Value::Integer(glyph_type),
                Value::Nil,
                Value::Nil,
            ]))
        })?,
    )?;
    g.set(
        "GetGlyphLink",
        lua.create_function(|lua, _: MultiValue| Ok(Value::String(lua.create_string("")?)))?,
    )?;
    g.set(
        "GlyphMatchesSocket",
        lua.create_function(|_, _: MultiValue| Ok(false))?,
    )?;
    g.set(
        "PlaceGlyphInSocket",
        lua.create_function(|_, _: MultiValue| Ok(()))?,
    )?;
    g.set(
        "RemoveGlyphFromSocket",
        lua.create_function(|_, _: MultiValue| Ok(()))?,
    )?;
    g.set(
        "SpellCanTargetGlyph",
        lua.create_function(|_, _: MultiValue| Ok(false))?,
    )?;

    Ok(())
}

/// Register `GameTooltip:SetTalent(tab, index)` into the tooltip method table.
pub(super) fn install_tooltip_method(lua: &Lua, m: &Table) -> mlua::Result<()> {
    m.set(
        "SetTalent",
        lua.create_function(|lua, (this, args): (Table, MultiValue)| {
            // `is-number` on args 2 and 3 else `Usage: SetTalent(tabIndex, talentIndex)`
            // (`0x85555c`, `0x535394`).
            let tab = args.front().cloned().unwrap_or(Value::Nil);
            let i = args.get(1).cloned().unwrap_or(Value::Nil);
            let tab = binding_abi::index_arg(lua, tab, "Usage: SetTalent(tabIndex, talentIndex)")?;
            let i = binding_abi::index_arg(lua, i, "Usage: SetTalent(tabIndex, talentIndex)")?;
            let (display, ranks, reqs, mut lines) = {
                let model = lua.app_data_ref::<Model>().expect("model app_data");
                let Some(t) = talent_at(&model, tab, i) else {
                    return Ok(());
                };
                // From the ask-once spell store; a miss shows on the next hover.
                let next_desc = (t.next_spell != 0)
                    .then(|| {
                        model
                            .spell_tooltips
                            .get(&t.next_spell)
                            .map(|v| v.description.clone())
                    })
                    .flatten();
                (
                    t.display_spell,
                    (t.rank, t.max_rank),
                    t.reqs.clone(),
                    TalentLines {
                        exceptional: t.exceptional,
                        rank_line: None,
                        reqs: Vec::new(),
                        next_spell: t.next_spell,
                        next_desc,
                        learn: t.learnable,
                    },
                )
            };
            // `TOOLTIP_TALENT_RANK`, "Rank %d/%d" (`0x854a2c`, pushed at `0x52b213`); an
            // install without it shows no rank line. A GlobalStrings read runs an `_G` `__index`
            // (`0x703bf0`), which may call any verb, so it is made once the Model is released.
            lines.rank_line = crate::strings::global(lua, "TOOLTIP_TALENT_RANK").map(|tmpl| {
                crate::strings::fill(
                    &tmpl,
                    &[
                        crate::strings::Arg::D(ranks.0.into()),
                        crate::strings::Arg::D(ranks.1.into()),
                    ],
                )
            });
            // Then the requirement lines' templates, read the same way (`0x52b390`).
            lines.reqs = reqs.iter().filter_map(|r| r.line(lua)).collect();
            super::tooltip_spell::set_spell_with_talent(lua, &this, display, lines)
        })?,
    )?;
    Ok(())
}

/// Ask the spell store for the next rank's description; a miss queues it as a primary view's does.
pub(super) fn ask_next_rank(lua: &Lua, next_spell: u32) {
    if next_spell != 0 {
        let _ = spell_view_of(lua, next_spell, false); // a miss records the ask as a side effect
    }
}
