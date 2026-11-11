//! `SendAddonMessage(prefix, message [, distribution])`, the addon-to-addon channel. 1.12 has no
//! addon opcode: the binding (`0x49f920`) sends `prefix`, a TAB and `message` as one
//! `CMSG_MESSAGECHAT` string with language `LANG_ADDON` (`0xFFFFFFFF`), the only mark of addon
//! data on either side, and the receiver splits on the first TAB (`0x49a8d0`). The binding reads
//! Lua arguments 1 to 3 only: 1.12 has no whispered addon message.
//!
//! Deviation: the reference drops a send that resolves to PARTY while party slot 0's guid
//! (`0xbc6f48`) is zero (`0x49fa9f`-`0x49faab`); benilla sends it, because `PartyState` is empty
//! until the first `SMSG_GROUP_LIST` and the gate would swallow a real broadcast. vmangos drops a
//! group-less party line anyway (`ChatHandler.cpp:476`).

use mlua::{Lua, Value};

use super::Model;

/// The four distributions the binding sends on (whitelist `0x49fa3f`-`0x49fa4e`); the receiving
/// side's jump table (`0x49afe0`) names the same four. vmangos also accepts `LANG_ADDON` on the
/// officer, raid-leader, raid-warning, battleground-leader and channel lanes
/// (`ChatHandler.cpp:78`), where the 1.12 client never sends it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AddonDistribution {
    /// Chat type `0x01`; the default when the third argument is absent or not a string
    /// (`0x49fa06`).
    Party,
    /// Chat type `0x02`; sent as PARTY outside a raid.
    Raid,
    /// Chat type `0x03`.
    Guild,
    /// Chat type `0x5C`.
    Battleground,
}

impl AddonDistribution {
    /// A Lua distribution token, case-insensitive like the reference's lookup (`0x64a4c0` over the
    /// table at `0x49f7a0`). `None` raises `Unknown addon chat type`, the one error the reference
    /// gives an unknown token and a chat type outside the four alike (`0x49fa53`).
    pub fn from_token(token: &str) -> Option<Self> {
        for (name, dist) in [
            ("PARTY", Self::Party),
            ("RAID", Self::Raid),
            ("GUILD", Self::Guild),
            ("BATTLEGROUND", Self::Battleground),
        ] {
            if token.eq_ignore_ascii_case(name) {
                return Some(dist);
            }
        }
        None
    }

    /// The token the receiving side reports as `CHAT_MSG_ADDON`'s third argument (`0x49afe0`).
    pub fn token(self) -> &'static str {
        match self {
            Self::Party => "PARTY",
            Self::Raid => "RAID",
            Self::Guild => "GUILD",
            Self::Battleground => "BATTLEGROUND",
        }
    }

    /// RAID outside a raid silently goes out as PARTY (`0x49fa8b`-`0x49fa93`); vmangos drops a raid
    /// line from a non-raid group (`ChatHandler.cpp:522`). The reference tests the raid member
    /// count at `0xb713e0`, the cell `GetNumRaidMembers` (`0x4bb530`) returns, so `in_raid` is
    /// `PartyState::raid` being non-empty.
    pub fn effective(self, in_raid: bool) -> Self {
        match self {
            Self::Raid if !in_raid => Self::Party,
            other => other,
        }
    }
}

/// One addon broadcast, composed at the binding as the reference's is (`_snprintf` at
/// `0x49f9b3`) and sent by the host inside the call ([`super::ScriptCall::AddonMessage`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddonSend {
    /// `prefix`, TAB, `message`, cut at 2047 bytes. vmangos passes a `LANG_ADDON` line through
    /// unsanitised (`ChatHandler.cpp:49`), so the TAB reaches the receiver.
    pub text: String,
    /// The lane to send on, after the outside-a-raid downgrade.
    pub distribution: AddonDistribution,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddonWhisper {
    pub text: String,
    pub target: String,
}

fn install_wrath(lua: &Lua) -> mlua::Result<()> {
    lua.globals().set("SendAddonMessage", lua.create_function(
        |lua, (prefix, message, distribution, target): (Value, Value, Value, Value)| {
            let prefix = lua_tostring(lua, &prefix)?;
            let message = lua_tostring(lua, &message)?;
            if prefix.is_empty() && message.is_empty() {
                return Err(mlua::Error::runtime("Usage: SendAddonMessage(prefix, message [, distribution [, target]])"));
            }
            if prefix.contains('\t') {
                return Err(mlua::Error::runtime("Addon prefix cannot contain a tab"));
            }
            let distribution = lua.coerce_string(distribution)?
                .map_or_else(|| "PARTY".to_owned(), |value| value.to_string_lossy());
            // WMBridge composes its HELLO line before `sendPayload` attempts CHANNEL and falls
            // back to WHISPER (`WMBridge.lua:135-175`). When a fallback whisper carries a stale
            // `transport=CHANNEL` tag, rewrite it to the actual `transport=SELF_WHISPER` lane.
            let message = if distribution.eq_ignore_ascii_case("WHISPER")
                && prefix == "WMBridge"
                && message.contains("transport=CHANNEL")
            {
                message.replace("transport=CHANNEL", "transport=SELF_WHISPER")
            } else {
                message
            };
            let text = format!("{prefix}\t{message}");
            if distribution.eq_ignore_ascii_case("WHISPER") {
                let target = lua_tostring(lua, &target)?;
                if target.is_empty() {
                    return Err(mlua::Error::runtime("Addon whisper missing target player"));
                }
                // `WM_SpellmakingV3.lua:36` calls `SendAddonMessage("WM_FORGE3", ..., "WHISPER", target)`
                // without `pcall`. When an upload chunk or graph line exceeds the 255-byte server
                // cap, split it into <=150-byte `DATA|` (or prefix-bounded) whisper frames rather
                // than aborting the Spellwright Lua flow.
                if text.len() > 255 && prefix == "WM_FORGE3" {
                    let body = message.strip_prefix("DATA|").unwrap_or(&message);
                    let frame_prefix = if message.starts_with("DATA|") {
                        "WM_FORGE3\tDATA|"
                    } else {
                        "WM_FORGE3\t"
                    };
                    let max_chunk = (255usize.saturating_sub(frame_prefix.len())).min(150).max(1);
                    let mut rest = body;
                    while !rest.is_empty() {
                        let mut cut = rest.len().min(max_chunk);
                        while cut > 0 && !rest.is_char_boundary(cut) {
                            cut -= 1;
                        }
                        let chunk = &rest[..cut];
                        super::calls::submit(
                            lua,
                            super::ScriptCall::AddonWhisper(AddonWhisper {
                                text: format!("{frame_prefix}{chunk}"),
                                target: target.clone(),
                            }),
                        )?;
                        rest = &rest[cut..];
                    }
                    return Ok(());
                }
                if text.len() > 255 {
                    return Err(mlua::Error::runtime("Addon prefix and message exceed the server's 255-byte limit"));
                }
                return super::calls::submit(lua, super::ScriptCall::AddonWhisper(AddonWhisper { text, target }));
            }
            // The installed AzerothCore ChatHandler rejects messages past 255 bytes. Refuse
            // an oversized structured message rather than silently truncating its payload.
            if text.len() > 255 {
                return Err(mlua::Error::runtime("Addon prefix and message exceed the server's 255-byte limit"));
            }
            // CHANNEL is intentionally not admitted: the installed server's LANG_ADDON
            // whitelist excludes it. WMBridge can then take its explicit WHISPER fallback.
            let distribution = AddonDistribution::from_token(&distribution)
                .ok_or_else(|| mlua::Error::runtime("Unknown addon chat type"))?;
            super::calls::submit(lua, super::ScriptCall::AddonMessage(AddonSend { text, distribution }))
        },
    )?)
}

/// `_snprintf(dst, 0x800, …)` writes at most 2047 bytes plus its NUL (`0x64a861`-`0x64a868`).
const ADDON_MESSAGE_CAP: usize = 0x800 - 1;

/// `lua_tostring` (`0x49f958`, `0x49f966`) with the reference's NULL-to-empty replacement
/// (`0x49f96b`-`0x49f97a`): strings and numbers coerce, anything else is `""`. The client reads the
/// result as a C string, so it ends at its first NUL: the usage test reads each first byte
/// (`0x49f97f`, `0x49f984`) and the `%s` of the compose stops there (`0x49f9a2`).
fn lua_tostring(lua: &Lua, v: &Value) -> mlua::Result<String> {
    Ok(lua
        .coerce_string(v.clone())?
        .map(|s| {
            let bytes = s.as_bytes();
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
            String::from_utf8_lossy(&bytes[..end]).into_owned()
        })
        .unwrap_or_default())
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    if lua.app_data_ref::<Model>().expect("model").target == super::UiTarget::Wrath335 {
        return install_wrath(lua);
    }
    lua.globals().set(
        "SendAddonMessage",
        lua.create_function(
            |lua, (prefix, message, distribution): (Value, Value, Value)| {
                let prefix = lua_tostring(lua, &prefix)?;
                let message = lua_tostring(lua, &message)?;
                // Only both empty is the usage error (`0x49f97f`-`0x49f987`): AceEvent-2.0 sends
                // `SendAddonMessage("LOOT_OPENED", "", "RAID")`.
                if prefix.is_empty() && message.is_empty() {
                    return Err(mlua::Error::RuntimeError(
                        r#"Usage: SendAddonMessage("prefix", "message" [,"type"])"#.into(),
                    ));
                }
                let mut text = format!("{prefix}\t{message}");
                // Cut at 2047 bytes as `_snprintf` does. Deviation: backed off to a char boundary,
                // because a `String` must stay UTF-8; the cut is the same for ASCII.
                if text.len() > ADDON_MESSAGE_CAP {
                    let mut cut = ADDON_MESSAGE_CAP;
                    while cut > 0 && !text.is_char_boundary(cut) {
                        cut -= 1;
                    }
                    text.truncate(cut);
                }
                // The escape scan (`0x49f9bb`-`0x49fa04`), its only text check: after the compose,
                // before argument 3 is read (`0x49fa06` precedes `0x49fa0b`), so a bad escape is
                // reported over a bad distribution.
                if super::chat_send::validate_escapes(&mut text).is_err() {
                    return Err(mlua::Error::runtime(super::chat_send::INVALID_ESCAPE));
                }
                // `lua_isstring` (`0x49fa15`) is true for a number, so `1` is the token "1" and
                // raises; anything else that is not a string is PARTY (`0x49fa1c`).
                let asked = match lua.coerce_string(distribution)? {
                    Some(token) => AddonDistribution::from_token(&token.to_string_lossy())
                        .ok_or_else(|| {
                            mlua::Error::RuntimeError("Unknown addon chat type".into())
                        })?,
                    None => AddonDistribution::Party,
                };
                let in_raid = {
                    let model = lua.app_data_ref::<Model>().expect("model app_data");
                    !model.party.raid.is_empty()
                };
                super::calls::submit(
                    lua,
                    super::ScriptCall::AddonMessage(AddonSend {
                        text,
                        distribution: asked.effective(in_raid),
                    }),
                )
            },
        )?,
    )?;
    Ok(())
}

impl super::UiScript {
    /// The `SendAddonMessage` broadcasts a harness's VM kept ([`Self::keep_calls`]), in call order.
    pub fn take_addon_sends(&mut self) -> Vec<AddonSend> {
        self.take_calls_where(|call| match call {
            super::ScriptCall::AddonMessage(send) => Some(send.clone()),
            _ => None,
        })
    }

    /// The `SendAddonMessage(..., "WHISPER", target)` sends a harness's VM kept, in call order.
    pub fn take_addon_whispers(&mut self) -> Vec<AddonWhisper> {
        self.take_calls_where(|call| match call {
            super::ScriptCall::AddonWhisper(whisper) => Some(whisper.clone()),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::party::{PartyState, RaidMemberInfo};
    use crate::script::UiScript;

    #[test]
    fn a_broadcast_is_prefix_tab_message_and_defaults_to_party() {
        let mut s = UiScript::new().unwrap();
        s.keep_calls();
        s.run(r#"SendAddonMessage("oRA", "hello")"#).unwrap();
        assert_eq!(
            s.take_addon_sends(),
            vec![AddonSend {
                text: "oRA\thello".into(),
                distribution: AddonDistribution::Party,
            }]
        );
        assert!(s.take_addon_sends().is_empty());
    }

    /// `prefix` and `message` are C strings (`0x49f958`, `0x49f966`), so each ends at its first
    /// NUL: the compose joins what is before it (`0x49f9a2`), the escape scan sees only that, and
    /// the usage test reads each first byte (`0x49f97f`, `0x49f984`).
    #[test]
    fn prefix_and_message_end_at_their_first_nul() {
        let mut s = UiScript::new().unwrap();
        s.keep_calls();
        s.run(
            r#"SendAddonMessage("a\0b", "c")
            SendAddonMessage("a\0|", "m")
            SendAddonMessage("p", "x\0|z")"#,
        )
        .unwrap();
        let texts: Vec<String> = s.take_addon_sends().into_iter().map(|a| a.text).collect();
        assert_eq!(texts, ["a\tc", "a\tm", "p\tx"]);
        let err = s
            .run(r#"SendAddonMessage("\0x", "")"#)
            .expect_err("both empty as C strings");
        assert!(err.to_string().contains("Usage: SendAddonMessage"), "{err}");
        assert!(s.take_addon_sends().is_empty());
    }

    #[test]
    fn the_distribution_set_is_exactly_party_raid_guild_battleground() {
        let mut s = UiScript::new().unwrap();
        s.keep_calls();
        // In a raid, so RAID is not downgraded.
        s.set_party(PartyState {
            raid: vec![RaidMemberInfo::default()],
            ..Default::default()
        });
        for (token, want) in [
            ("PARTY", AddonDistribution::Party),
            ("RAID", AddonDistribution::Raid),
            ("GUILD", AddonDistribution::Guild),
            ("BATTLEGROUND", AddonDistribution::Battleground),
            ("raid", AddonDistribution::Raid),
            ("Guild", AddonDistribution::Guild),
        ] {
            s.run(&format!(r#"SendAddonMessage("p", "m", "{token}")"#))
                .unwrap_or_else(|e| panic!("{token} must send: {e}"));
            let sent = s.take_addon_sends();
            assert_eq!(sent.len(), 1, "{token}");
            assert_eq!(sent[0].distribution, want, "{token}");
        }
        // Chat types the reference's table resolves (`0x49f7a0`) but the whitelist refuses, and
        // unknown tokens: one error for both.
        for token in [
            "SAY",
            "YELL",
            "WHISPER",
            "EMOTE",
            "OFFICER",
            "CHANNEL",
            "AFK",
            "DND",
            "RAID_WARNING",
            "RAID_LEADER",
            "BATTLEGROUND_LEADER",
            "NOT_A_TYPE",
            "",
        ] {
            let err = s
                .run(&format!(r#"SendAddonMessage("p", "m", "{token}")"#))
                .expect_err(&format!("{token} must be refused, not guessed"));
            assert!(
                err.to_string().contains("Unknown addon chat type"),
                "{token}: {err}"
            );
            assert!(
                s.take_addon_sends().is_empty(),
                "{token} must queue nothing"
            );
        }
    }

    #[test]
    fn a_number_distribution_raises_rather_than_being_read_as_a_type_byte() {
        let mut s = UiScript::new().unwrap();
        s.keep_calls();
        let err = s
            .run(r#"SendAddonMessage("p", "m", 1)"#)
            .expect_err("a numeric distribution must raise");
        assert!(err.to_string().contains("Unknown addon chat type"), "{err}");
        assert!(s.take_addon_sends().is_empty());
    }

    #[test]
    fn a_non_string_distribution_takes_the_party_default() {
        let mut s = UiScript::new().unwrap();
        s.keep_calls();
        s.run(r#"SendAddonMessage("p", "m", nil)"#).unwrap();
        s.run(r#"SendAddonMessage("p", "m", {})"#).unwrap();
        let sent = s.take_addon_sends();
        assert_eq!(sent.len(), 2);
        assert!(sent
            .iter()
            .all(|a| a.distribution == AddonDistribution::Party));
    }

    #[test]
    fn raid_outside_a_raid_is_downgraded_to_party() {
        let mut s = UiScript::new().unwrap();
        s.keep_calls();
        // Default PartyState: not in a raid.
        s.run(r#"SendAddonMessage("CTRA", "status", "RAID")"#)
            .unwrap();
        assert_eq!(
            s.take_addon_sends()[0].distribution,
            AddonDistribution::Party
        );
        s.set_party(PartyState {
            raid: vec![RaidMemberInfo::default()],
            ..Default::default()
        });
        s.run(r#"SendAddonMessage("CTRA", "status", "RAID")"#)
            .unwrap();
        assert_eq!(
            s.take_addon_sends()[0].distribution,
            AddonDistribution::Raid
        );
        // Only RAID is downgraded.
        s.set_party(PartyState::default());
        for (token, want) in [
            ("GUILD", AddonDistribution::Guild),
            ("BATTLEGROUND", AddonDistribution::Battleground),
        ] {
            s.run(&format!(r#"SendAddonMessage("p", "m", "{token}")"#))
                .unwrap();
            assert_eq!(s.take_addon_sends()[0].distribution, want, "{token}");
        }
    }

    #[test]
    fn only_both_empty_is_a_usage_error() {
        let mut s = UiScript::new().unwrap();
        s.keep_calls();
        let err = s
            .run(r#"SendAddonMessage("", "")"#)
            .expect_err("both empty must raise");
        assert!(err.to_string().contains("Usage: SendAddonMessage"), "{err}");
        assert!(s.take_addon_sends().is_empty());
        // AceEvent-2.0's own call.
        s.run(r#"SendAddonMessage("LOOT_OPENED", "", "RAID")"#)
            .unwrap();
        s.run(r#"SendAddonMessage("", "m")"#).unwrap();
        let sent = s.take_addon_sends();
        assert_eq!(sent[0].text, "LOOT_OPENED\t");
        assert_eq!(sent[1].text, "\tm");
    }

    #[test]
    fn arguments_coerce_like_lua_tostring_and_nil_becomes_empty() {
        let mut s = UiScript::new().unwrap();
        s.keep_calls();
        s.run(r#"SendAddonMessage("v", 314)"#).unwrap();
        assert_eq!(s.take_addon_sends()[0].text, "v\t314");
        s.run(r#"SendAddonMessage(42, "m")"#).unwrap();
        assert_eq!(s.take_addon_sends()[0].text, "42\tm");
        let err = s
            .run(r#"SendAddonMessage({}, {})"#)
            .expect_err("two tables coerce to two empty strings");
        assert!(err.to_string().contains("Usage: SendAddonMessage"), "{err}");
    }

    /// The reference does not strip a TAB from the prefix either; the receiver splits at it.
    #[test]
    fn a_tab_inside_the_prefix_is_passed_through_unsanitised() {
        let mut s = UiScript::new().unwrap();
        s.keep_calls();
        s.run("SendAddonMessage(\"pre\\tfix\", \"body\")").unwrap();
        assert_eq!(s.take_addon_sends()[0].text, "pre\tfix\tbody");
    }

    #[test]
    fn the_payload_truncates_silently_at_2047_bytes() {
        let mut s = UiScript::new().unwrap();
        s.keep_calls();
        s.run(r#"SendAddonMessage("p", string.rep("x", 4000))"#)
            .unwrap();
        let sent = s.take_addon_sends();
        assert_eq!(sent[0].text.len(), 2047);
        assert!(sent[0].text.starts_with("p\tx"));
        // 1 + 1 + 2045 bytes fills the buffer exactly and is not cut.
        s.run(r#"SendAddonMessage("p", string.rep("y", 2045))"#)
            .unwrap();
        assert_eq!(s.take_addon_sends()[0].text.len(), 2047);
    }

    #[test]
    fn a_well_formed_colour_run_passes_and_its_closer_is_skipped_not_scanned() {
        let mut s = UiScript::new().unwrap();
        s.keep_calls();
        for payload in [
            "|cffff0000red|r",
            "before |cffff0000red|r after",
            // Nothing between `|c` and the first `|r` is examined, so a nested open passes.
            "|cffAA0000 a |cff00BB00 b |r",
            // A link passes only inside a colour run.
            "|cffa335ee|Hitem:12345:0:0:0|h[Thunderfury]|h|r",
            // Escaped pipes come in even runs.
            "a || b",
            "||||",
        ] {
            s.run(&format!(
                "SendAddonMessage(\"P\", {:?}, \"GUILD\")",
                payload
            ))
            .unwrap_or_else(|e| panic!("{payload:?} must send: {e}"));
            let sent = s.take_addon_sends();
            assert_eq!(sent.len(), 1, "{payload:?}");
            assert_eq!(
                sent[0].text,
                format!("P\t{payload}"),
                "{payload:?} verbatim"
            );
        }
    }

    #[test]
    fn every_other_escape_start_raises_including_a_bare_closer_and_a_trailing_pipe() {
        let mut s = UiScript::new().unwrap();
        s.keep_calls();
        for payload in [
            "|r",                    // a closer with nothing open
            "|Hitem:1:0:0:0|h[x]|h", // link tokens outside a colour run
            "|h",
            "|T",
            "|t",
            "|n",
            "|C",         // |C is not |c: the test is case-sensitive
            "trailing |", // next byte is the NUL
            "|||",        // odd pipe run: the third one's next byte is the NUL
        ] {
            let err = s
                .run(&format!(
                    "SendAddonMessage(\"P\", {:?}, \"GUILD\")",
                    payload
                ))
                .expect_err(&format!("{payload:?} must raise"));
            assert!(
                err.to_string()
                    .contains("Invalid escape code in chat message"),
                "{payload:?}: {err}"
            );
            assert!(
                s.take_addon_sends().is_empty(),
                "{payload:?} queues nothing"
            );
        }
    }

    #[test]
    fn an_unmatched_colour_open_truncates_in_place_and_still_sends() {
        let mut s = UiScript::new().unwrap();
        s.keep_calls();
        s.run(r#"SendAddonMessage("P", "keep|cff00ff00dropped", "GUILD")"#)
            .unwrap();
        assert_eq!(s.take_addon_sends()[0].text, "P\tkeep");
        // `|R` does not close `|c`: the reference's `strstr` is case-sensitive.
        s.run(r#"SendAddonMessage("P", "keep|cff00ff00x|Ry", "GUILD")"#)
            .unwrap();
        assert_eq!(s.take_addon_sends()[0].text, "P\tkeep");
        // The scan runs on the composed text: an open in the prefix cuts the TAB and message off.
        s.run(r#"SendAddonMessage("pre|cffAABBCCfix", "body", "GUILD")"#)
            .unwrap();
        assert_eq!(s.take_addon_sends()[0].text, "pre");
    }

    #[test]
    fn a_colour_run_may_span_the_tab_because_the_scan_is_on_the_composed_buffer() {
        let mut s = UiScript::new().unwrap();
        s.keep_calls();
        s.run(r#"SendAddonMessage("|cffff0000pre", "post|r", "GUILD")"#)
            .unwrap();
        assert_eq!(s.take_addon_sends()[0].text, "|cffff0000pre\tpost|r");
    }

    #[test]
    fn a_bad_escape_beats_a_bad_distribution() {
        let s = UiScript::new().unwrap();
        let err = s
            .run(r#"SendAddonMessage("P", "|z", "TOTALLY_BOGUS")"#)
            .expect_err("must raise");
        assert!(
            err.to_string()
                .contains("Invalid escape code in chat message"),
            "the escape must win, not the type: {err}"
        );
        // An unmatched `|c` is not an error, so the bad distribution is reported.
        let err = s
            .run(r#"SendAddonMessage("P", "|cff000000x", "TOTALLY_BOGUS")"#)
            .expect_err("must raise");
        assert!(err.to_string().contains("Unknown addon chat type"), "{err}");
    }

    #[test]
    fn the_tokens_are_the_receive_sides_own_strings() {
        for (dist, token) in [
            (AddonDistribution::Party, "PARTY"),
            (AddonDistribution::Raid, "RAID"),
            (AddonDistribution::Guild, "GUILD"),
            (AddonDistribution::Battleground, "BATTLEGROUND"),
        ] {
            assert_eq!(dist.token(), token);
            assert_eq!(AddonDistribution::from_token(token), Some(dist));
        }
    }

    #[test]
    fn wmbridge_self_whisper_hello_and_kill_events() {
        use crate::script::{CombatLogPartyKill, CombatLogUnit, UiTarget};

        let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        s.keep_calls();

        // 1. An unconfirmed JoinTemporaryChannel/JoinChannelByName in Wrath335 does not report a
        // confirmed channel id, and a pre-composed HELLO falling back from CHANNEL to WHISPER
        // reports transport=SELF_WHISPER.
        s.run(
            r#"
            JoinTemporaryChannel("WMBridge")
            PendingChannelId = GetChannelName("WMBridge")
            local msg = "HELLO|version=1|player=Firna|transport=CHANNEL"
            local ok = pcall(SendAddonMessage, "WMBridge", msg, "CHANNEL", 1)
            ChannelSendSucceeded = ok
            if not ok then
                SendAddonMessage("WMBridge", msg, "WHISPER", "Firna")
            end
            "#,
        )
        .unwrap();

        assert_eq!(s.eval::<i64>("return PendingChannelId").unwrap(), 0);
        assert!(!s.eval::<bool>("return ChannelSendSucceeded").unwrap());
        let whispers = s.take_addon_whispers();
        assert_eq!(
            whispers,
            vec![AddonWhisper {
                text: "WMBridge\tHELLO|version=1|player=Firna|transport=SELF_WHISPER".to_owned(),
                target: "Firna".to_owned(),
            }]
        );

        // 2. Both COMBAT_LOG_EVENT and COMBAT_LOG_EVENT_UNFILTERED are admitted in Wrath335, and
        // PARTY_KILL dispatches the 8-argument event WMBridge consumes.
        s.run(
            r#"
            local f = CreateFrame("Frame")
            f:RegisterEvent("COMBAT_LOG_EVENT")
            f:RegisterEvent("COMBAT_LOG_EVENT_UNFILTERED")
            f:SetScript("OnEvent", function(self, event, ts, subevent, srcGuid, srcName, srcFlags, dstGuid, dstName, dstFlags)
                if event == "COMBAT_LOG_EVENT_UNFILTERED" and subevent == "PARTY_KILL" then
                    SendAddonMessage("WMBridge", string.format("KILL|src=%s|dst=%s|%s", srcGuid, dstGuid, dstName), "WHISPER", "Firna")
                end
            end)
            "#,
        )
        .unwrap();

        s.fire_combat_log_party_kill(&CombatLogPartyKill {
            timestamp: 1_700_000_000.0,
            source: CombatLogUnit {
                guid: 0x0000_0000_0000_0001,
                name: Some("Firna".to_owned()),
                flags: 0x0511,
            },
            dest: CombatLogUnit {
                guid: 0xF130_0000_2F00_002A,
                name: Some("Kobold Vermin".to_owned()),
                flags: 0x0a48,
            },
        });

        let kill_whispers = s.take_addon_whispers();
        assert_eq!(
            kill_whispers,
            vec![AddonWhisper {
                text: "WMBridge\tKILL|src=0x0000000000000001|dst=0xF13000002F00002A|Kobold Vermin"
                    .to_owned(),
                target: "Firna".to_owned(),
            }]
        );
    }

    #[test]
    fn wm_spellmaking_self_whisper_round_trip() {
        use crate::script::{ScriptValue, UiTarget};

        let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        s.keep_calls();

        // Simulate `WM_SpellmakingV3.lua:36` (`SendAddonMessage("WM_FORGE3", payload, "WHISPER", self)`)
        // without `pcall`, including an oversized `DATA|` graph payload that chunks into <=150-byte
        // `WM_FORGE3\tDATA|...` whispers, followed by an inbound `QUOTE|<nonce>` reply that triggers
        // `BUY|<nonce>`.
        s.run(
            r#"
            local f = CreateFrame("Frame")
            f:RegisterEvent("CHAT_MSG_ADDON")
            f:SetScript("OnEvent", function(self, event, prefix, msg, dist, sender)
                if event == "CHAT_MSG_ADDON" and prefix == "WM_FORGE3" then
                    local nonce = string.match(msg, "^QUOTE|([%w_%-]+)")
                    if nonce then
                        SendAddonMessage("WM_FORGE3", "BUY|" .. nonce, "WHISPER", sender)
                    end
                end
            end)

            SendAddonMessage("WM_FORGE3", "LIST", "WHISPER", "Firna")
            SendAddonMessage("WM_FORGE3", "BEGIN|QUOTE", "WHISPER", "Firna")
            local graph = "DATA|" .. string.rep("node:cast_bolt->arc_chain;", 12)
            SendAddonMessage("WM_FORGE3", graph, "WHISPER", "Firna")
            SendAddonMessage("WM_FORGE3", "FINISH|QUOTE", "WHISPER", "Firna")
            "#,
        )
        .unwrap();

        let sent = s.take_addon_whispers();
        assert!(
            sent.len() >= 5,
            "oversized DATA| payload must split into bounded whispers: {sent:?}"
        );
        assert_eq!(sent[0].text, "WM_FORGE3\tLIST");
        assert_eq!(sent[1].text, "WM_FORGE3\tBEGIN|QUOTE");
        for chunk in &sent[2..sent.len() - 1] {
            assert!(chunk.text.starts_with("WM_FORGE3\tDATA|"));
            assert!(
                chunk.text.len() <= 255,
                "each chunk must stay within the 255-byte server cap: {}",
                chunk.text.len()
            );
            assert_eq!(chunk.target, "Firna");
        }
        assert_eq!(sent.last().unwrap().text, "WM_FORGE3\tFINISH|QUOTE");

        s.fire_event(
            "CHAT_MSG_ADDON",
            vec![
                ScriptValue::Str("WM_FORGE3".to_owned()),
                ScriptValue::Str("QUOTE|nonce_9001|cost=250".to_owned()),
                ScriptValue::Str("WHISPER".to_owned()),
                ScriptValue::Str("Firna".to_owned()),
            ],
        );

        let buy = s.take_addon_whispers();
        assert_eq!(
            buy,
            vec![AddonWhisper {
                text: "WM_FORGE3\tBUY|nonce_9001".to_owned(),
                target: "Firna".to_owned(),
            }]
        );
    }
}
