//! Wrath-native edges into the stock UI event contract. The net reducer remains authoritative;
//! the only retained state here is bounded display metadata for terminal/scope-loss cleanup.

use std::collections::BTreeMap;

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use benilla_ui::script::{ScriptValue, UiScript, UiTarget};

use crate::wrath_native::{WrathCastEvent, WrathCastEventKind as Kind};

#[derive(Clone)]
struct DisplayedCast {
    tokens: Vec<String>,
    name: String,
    rank: String,
}

#[derive(Default)]
pub(crate) struct CastMemo {
    vm: Option<u64>,
    scope: Option<(u64, u64)>,
    // Caster plus channel/cast, since a channel START can precede the cast's final edge.
    displayed: BTreeMap<(u64, bool, Option<u64>), DisplayedCast>,
}

fn tokens(script: &UiScript, guid: u64) -> Vec<String> {
    if guid == 0 { return Vec::new(); }
    let units = script.unit_guids_view();
    let mut result = Vec::new();
    for (token, current) in [("player", units.player), ("pet", units.pet),
        ("target", units.target), ("mouseover", units.mouseover), ("npc", units.npc)]
    {
        if current == guid { result.push(token.to_owned()); }
    }
    for (prefix, members) in [("party", units.party.as_slice()), ("partypet", units.party_pets.as_slice()),
        ("raid", units.raid.as_slice()), ("raidpet", units.raid_pets.as_slice())]
    {
        for (index, &current) in members.iter().enumerate() {
            if current == guid { result.push(format!("{prefix}{}", index + 1)); }
        }
    }
    result
}

pub(crate) fn feed(
    mut vm: crate::script_calls::VmWith<(
        Option<Res<'static, Messages<WrathCastEvent>>>,
        Option<Res<'static, crate::wrath::WrathWorldView>>,
        Option<Res<'static, crate::ui_action::Spells>>,
    )>,
    mut cursor: Local<MessageCursor<WrathCastEvent>>,
    mut memo: Local<CastMemo>,
) {
    vm.feed(|script, (messages, world, spells), fires| {
        // Drain even with no live VM, so a rebuilt VM does not replay old notifications.
        let events: Vec<_> = messages.as_deref().map_or_else(Vec::new,
            |messages| cursor.read(messages).cloned().collect());
        let Some(script) = script.filter(|script| script.target() == UiTarget::Wrath335) else {
            *memo = CastMemo::default();
            return;
        };
        if memo.vm != Some(script.session()) {
            *memo = CastMemo { vm: Some(script.session()), ..Default::default() };
        }
        let current = world.as_ref().map(|world| (world.session_epoch, world.world_generation));
        let in_world = world.as_ref().is_some_and(|world|
            world.phase == Some(benilla_protocol_wrath::session::Phase::InWorld));
        if in_world && memo.scope != current {
            memo.displayed.clear();
            memo.scope = current;
        }
        for event in events {
            let scope = (event.scope.session_epoch, event.scope.world_generation);
            let invalidated = matches!(&event.kind, Kind::ScopeInvalidated { .. });
            // A terminal may clear the departing scope after the unit projection was torn down.
            // It must never clear a different, already-adopted world's cast bar.
            if current != Some(scope) && !(invalidated && !in_world && memo.scope == Some(scope)) {
                continue;
            }
            if !in_world && !invalidated { continue; }
            if let Some(serial) = event.attempt_serial {
                if serial > (1u64 << 53) { continue; }
            }
            let Some(identity) = event.identity else { continue; };
            let channel = matches!(&event.kind, Kind::ChannelStarted { .. } | Kind::ChannelUpdated { .. }
                | Kind::ChannelEnded) || (invalidated && event.attempt_serial.is_none());
            let key = (identity.caster, channel, event.attempt_serial);
            let mut aliases = tokens(script, identity.caster);
            if invalidated && aliases.is_empty() && memo.scope == Some(scope) {
                if let Some(previous) = memo.displayed.get(&key) { aliases = previous.tokens.clone(); }
            }
            let definition = spells.as_deref().and_then(|spells| spells.catalog.get(identity.spell_id));
            let label = definition.map(|definition| (definition.name.clone(), definition.rank.clone().unwrap_or_default()))
                .or_else(|| memo.displayed.get(&key).map(|previous| (previous.name.clone(), previous.rank.clone())));
            let Some((name, rank)) = label else { continue; };
            let (names, terminal): (&[&str], bool) = match &event.kind {
                Kind::Started { .. } => (&["UNIT_SPELLCAST_START"], false),
                Kind::Delayed { .. } => (&["UNIT_SPELLCAST_DELAYED"], false),
                // GO establishes launch; SUCCEEDED is the client spellcast event, not hit/damage.
                Kind::Launched { .. } => (&["UNIT_SPELLCAST_SUCCEEDED", "UNIT_SPELLCAST_STOP"], true),
                Kind::Failed { .. } => (&["UNIT_SPELLCAST_FAILED"], true),
                Kind::Interrupted { .. } => (&["UNIT_SPELLCAST_INTERRUPTED"], true),
                Kind::ChannelStarted { .. } => (&["UNIT_SPELLCAST_CHANNEL_START"], false),
                Kind::ChannelUpdated { .. } => (&["UNIT_SPELLCAST_CHANNEL_UPDATE"], false),
                Kind::ChannelEnded => (&["UNIT_SPELLCAST_CHANNEL_STOP"], true),
                Kind::ScopeInvalidated { .. } if channel => (&["UNIT_SPELLCAST_CHANNEL_STOP"], true),
                Kind::ScopeInvalidated { .. } => (&["UNIT_SPELLCAST_STOP"], true),
            };
            for token in &aliases {
                let args = vec![ScriptValue::Str(token.clone()), ScriptValue::Str(name.clone()),
                    ScriptValue::Str(rank.clone()), event.attempt_serial.map_or(ScriptValue::Nil,
                        |serial| ScriptValue::Number(serial as f64))];
                for event in names { fires.post(event, args.clone()); }
            }
            if terminal { memo.displayed.remove(&key); }
            else if !aliases.is_empty() {
                // Display bookkeeping cannot grow with all world casters or reconnect history.
                if memo.displayed.len() >= 128 && !memo.displayed.contains_key(&key) {
                    memo.displayed.clear();
                }
                memo.displayed.insert(key, DisplayedCast { tokens: aliases, name, rank });
            }
        }
        if memo.scope != current { memo.displayed.clear(); }
        memo.scope = current;
    });
}
