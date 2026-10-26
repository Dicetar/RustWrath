//! Typed WM player-action intents and their authoritative client results.
//!
//! This is the action half of the same loopback seam as [`crate::Observation`]. WM proposes one
//! bounded intent; the client maps it onto its existing command owners and answers with the
//! authoritative outcome it actually observed. Queueing a command is not success: `Accepted`
//! means the intent reached a command owner, and terminal states come only from server replies or
//! session state.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Hard bound on one request id (printable ASCII).
pub const MAX_REQUEST_ID_CHARS: usize = 64;
/// Hard bound on the human-readable detail carried by one result.
pub const MAX_ACTION_DETAIL_CHARS: usize = 200;

/// One permitted player action. Variants mirror the existing `ClientCommand` owners; an intent
/// with no owner is answered `Unsupported` rather than being approximated.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActionIntent {
    /// Select a unit (`CMSG_SET_SELECTION`).
    SetTarget { guid: u64 },
    /// Start melee auto-attack (`CMSG_ATTACKSWING`).
    AttackStart { guid: u64 },
    /// Stop melee auto-attack (`CMSG_ATTACKSTOP`).
    AttackStop,
    /// Cast a known spell, optionally at a unit target (`CMSG_CAST_SPELL`).
    Cast { spell_id: u32, target: Option<u64> },
    /// Cancel the in-flight cast (`CMSG_CANCEL_CAST` / the stock cancel owner).
    CastCancel,
    /// Open a unit's gossip window (`CMSG_GOSSIP_HELLO`).
    Interact { guid: u64 },
    /// Choose one gossip option (`CMSG_GOSSIP_SELECT_OPTION`); the live menu id comes from the
    /// client's adopted gossip state, not from WM.
    GossipSelect { guid: u64, option: u32 },
    /// Use a game object (`CMSG_GAMEOBJ_USE`), such as a chest or door.
    UseObject { guid: u64 },
    /// Accept a quest offered by an NPC (`CMSG_QUESTGIVER_ACCEPT_QUEST`).
    QuestAccept { npc: u64, quest: u32 },
    /// Hand in a completed quest (`CMSG_QUESTGIVER_COMPLETE_QUEST`).
    QuestComplete { npc: u64, quest: u32 },
    /// Choose a quest reward by index (`CMSG_QUESTGIVER_CHOOSE_REWARD`).
    QuestChooseReward { npc: u64, quest: u32, reward: u32 },
    /// Take one loot slot (`CMSG_AUTOSTORE_LOOT_ITEM`).
    LootTake { source: u64, slot: u8 },
    /// Take the loot source's coin (`CMSG_LOOT_MONEY`).
    LootMoney { source: u64 },
    /// Release the loot window (`CMSG_LOOT_RELEASE`).
    LootRelease { source: u64 },
    /// No remote movement owner exists in the client (movement is the human input controller),
    /// so this intent is answered `Unsupported` with that reason.
    MoveStart { direction: MoveDirection },
    /// See [`Self::MoveStart`].
    MoveStop,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MoveDirection {
    Forward,
    Backward,
    StrafeLeft,
    StrafeRight,
    TurnLeft,
    TurnRight,
}

impl ActionIntent {
    /// Semantic validation beyond wire shape.
    pub fn validate(&self) -> Result<()> {
        let nonzero = |value: u64, name: &str| -> Result<()> {
            if value == 0 {
                Err(Error::InvalidAction(format!("{name} must be non-zero")))
            } else {
                Ok(())
            }
        };
        match self {
            Self::SetTarget { guid }
            | Self::AttackStart { guid }
            | Self::Interact { guid }
            | Self::UseObject { guid } => nonzero(*guid, "guid"),
            Self::GossipSelect { guid, .. } => nonzero(*guid, "guid"),
            Self::AttackStop | Self::CastCancel | Self::MoveStop => Ok(()),
            Self::Cast { spell_id, target } => {
                if *spell_id == 0 {
                    return Err(Error::InvalidAction("spell_id must be non-zero".to_owned()));
                }
                if let Some(target) = target {
                    nonzero(*target, "target")?;
                }
                Ok(())
            }
            Self::QuestAccept { npc, quest } | Self::QuestComplete { npc, quest } => {
                nonzero(*npc, "npc")?;
                if *quest == 0 {
                    return Err(Error::InvalidAction("quest must be non-zero".to_owned()));
                }
                Ok(())
            }
            Self::QuestChooseReward { npc, quest, .. } => {
                nonzero(*npc, "npc")?;
                if *quest == 0 {
                    return Err(Error::InvalidAction("quest must be non-zero".to_owned()));
                }
                Ok(())
            }
            Self::LootTake { source, .. } | Self::LootMoney { source } | Self::LootRelease { source } => {
                nonzero(*source, "source")
            }
            Self::MoveStart { .. } => Ok(()),
        }
    }

    /// Short stable name for diagnostics; never carries arguments.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::SetTarget { .. } => "set_target",
            Self::AttackStart { .. } => "attack_start",
            Self::AttackStop => "attack_stop",
            Self::Cast { .. } => "cast",
            Self::CastCancel => "cast_cancel",
            Self::Interact { .. } => "interact",
            Self::GossipSelect { .. } => "gossip_select",
            Self::UseObject { .. } => "use_object",
            Self::QuestAccept { .. } => "quest_accept",
            Self::QuestComplete { .. } => "quest_complete",
            Self::QuestChooseReward { .. } => "quest_choose_reward",
            Self::LootTake { .. } => "loot_take",
            Self::LootMoney { .. } => "loot_money",
            Self::LootRelease { .. } => "loot_release",
            Self::MoveStart { .. } => "move_start",
            Self::MoveStop => "move_stop",
        }
    }
}

/// One WM intent for the connected client. `request_id` is the broker's identity and is echoed
/// unchanged in the result; scope fields fence the intent against a retired session/world.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionRequest {
    pub request_id: String,
    pub session_epoch: u64,
    pub world_generation: u64,
    pub player_guid: u64,
    /// Wall-clock deadline; the client refuses to dispatch after it.
    pub deadline_unix_ms: i64,
    pub intent: ActionIntent,
}

impl ActionRequest {
    pub fn validate(&self) -> Result<()> {
        validate_request_id(&self.request_id)?;
        if self.session_epoch == 0 {
            return Err(Error::InvalidAction(
                "session_epoch must be non-zero".to_owned(),
            ));
        }
        if self.world_generation == 0 {
            return Err(Error::InvalidAction(
                "world_generation must be non-zero".to_owned(),
            ));
        }
        if self.player_guid == 0 {
            return Err(Error::InvalidAction(
                "player_guid must be non-zero".to_owned(),
            ));
        }
        if self.deadline_unix_ms < 0 {
            return Err(Error::InvalidAction(
                "deadline_unix_ms must not be negative".to_owned(),
            ));
        }
        self.intent.validate()
    }
}

/// Terminal or in-flight action state. `Accepted` is admission into a command owner and `Sent`
/// is the first authoritative witness that the command left the client; neither is gameplay
/// success. `Launched` is a cast that completed; it is not evidence of damage, quest progress or
/// any other gameplay outcome. `Unknown` is the only honest terminal state when the link failed
/// after a command may have been sent.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionStatus {
    Accepted,
    Sent,
    Launched,
    Succeeded,
    Failed,
    Rejected,
    Cancelled,
    Expired,
    Unsupported,
    Unknown,
}

impl ActionStatus {
    pub fn is_terminal(self) -> bool {
        !matches!(self, Self::Accepted | Self::Sent)
    }
}

/// How a result was correlated to the client-side action.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionCorrelation {
    /// The client bound the cast attempt at its START edge and matched the terminal edge to it.
    StartIdentity,
    /// The client could not bind a START (the terminal arrived without one, or the bounded
    /// presentation feed dropped it); the request was matched by per-spell wire order.
    WireOrder,
    /// Confirmed directly by a server reply that names the intent's own identity.
    Direct,
}

/// Authoritative evidence retained with a result.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionEvidence {
    /// The client's per-attempt handle, when the action bound one.
    pub attempt_serial: Option<u64>,
    /// 12340's `m_cast_count` observed on the matched edge.
    pub cast_count: Option<u8>,
    /// The terminal cast outcome that resolved the action, when this action is a cast.
    pub terminal: Option<crate::observation::CastOutcome>,
    pub correlation: ActionCorrelation,
}

/// The client's answer for one request. `detail` is short, printable and carries no credentials or
/// packet payloads.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionResult {
    pub request_id: String,
    pub status: ActionStatus,
    pub session_epoch: u64,
    pub world_generation: u64,
    /// The adopted world revision when the outcome was observed, when one exists.
    pub observed_state_revision: Option<u64>,
    /// The matching authoritative evidence, when the action has one.
    pub evidence: Option<ActionEvidence>,
    pub detail: Option<String>,
}

impl ActionResult {
    pub fn validate(&self) -> Result<()> {
        validate_request_id(&self.request_id)?;
        if let Some(evidence) = self.evidence.as_ref() {
            if evidence.attempt_serial == Some(0) {
                return Err(Error::InvalidAction(
                    "evidence attempt_serial must be non-zero".to_owned(),
                ));
            }
        }
        if let Some(detail) = self.detail.as_deref() {
            if detail.len() > MAX_ACTION_DETAIL_CHARS {
                return Err(Error::InvalidAction(
                    "result detail exceeds its character bound".to_owned(),
                ));
            }
            if !detail.chars().all(|ch| ch.is_ascii_graphic() || ch == ' ') {
                return Err(Error::InvalidAction(
                    "result detail must be printable ASCII".to_owned(),
                ));
            }
        }
        Ok(())
    }
}

fn validate_request_id(request_id: &str) -> Result<()> {
    if request_id.is_empty() || request_id.len() > MAX_REQUEST_ID_CHARS {
        return Err(Error::InvalidAction(
            "request_id must be 1..=64 characters".to_owned(),
        ));
    }
    if !request_id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err(Error::InvalidAction(
            "request_id must be ASCII alphanumeric, '-' or '_'".to_owned(),
        ));
    }
    Ok(())
}
