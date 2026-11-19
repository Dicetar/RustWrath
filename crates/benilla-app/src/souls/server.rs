//! The link to the Souls server script (`integrations/ale/rustwrath_souls.lua`).
//!
//! The channel is an addon whisper to oneself under the prefix [`PREFIX`]: the script swallows it
//! and answers with addon whispers from the player to the player, whose text the chat handler
//! hands to [`tap_addon`] instead of the addon log. Fields are tab-separated; guids are decimal.
//!
//! - **Mode.** While Souls combat is on and the `server` switch is on, `HELLO` and `MODE 1` go out,
//!   and `MODE 1` again every [`KEEPALIVE_SECONDS`] (a script reload forgets every player). The
//!   server's `HELLO`/`MODE 1` answer raises [`SoulsLink::up`]. Turning either off sends `MODE 0`,
//!   which hands the server's creatures back to stock melee and restores the run speed.
//! - **Hits.** With the link up, [`super::targets`] reports each hit window's struck units as
//!   `HIT <motion value %> <guid,...> <hands>` (nearest first; hands 2 when a one-handed weapon
//!   is held in both) instead of starting stock melee; the server applies the cleave and the
//!   Elden Ring damage: the weapon's roll times the motion value, the two-hand bonus, the counter
//!   bonus and the defense curve. The action's start goes out as `SWING <action>`, which the
//!   server relays to other Souls players, who play it on our body.
//! - **Sprint and crouch.** `SPRINT 1|0` and `CROUCH 1|0` follow them; the server raises or
//!   lowers the run speed, and shows the crouch to other players as the stealth pose.
//! - **Enemy strikes.** `STRIKE` telegraphs a creature's blow: a bar over it fills for the windup.
//!   A strike on us is judged at the end of the windup, from the creature's rendered facing (which
//!   the server locked) and our own state: out of reach or the cone a miss, inside a parry window
//!   a parry, inside a dodge's i-frames a dodge, behind a raised guard a guard, else a hit. The
//!   verdict goes back as `VERDICT`; the server applies the damage, which the playback's hit path
//!   then charges to the guard, poise or the running action. A parried creature opens a riposte
//!   window ([`SoulsLink::riposte`]): the next hit on it deals the riposte damage.
//! - **Stance.** The server counts each creature's stance down with our hits; at zero it sends
//!   `BROKEN`: the creature staggers (its stun stance) and the same riposte window opens.
//!
//! Everything resets the frame the mode or the switch is off.

use std::collections::VecDeque;
use std::sync::Mutex;

use bevy::prelude::*;

use super::input::{in_iframes, in_parry_window};
use super::targets::off_facing;
use super::{current_kit, running_action, souls_on, Instance, SoulsConfig, SoulsState};
use crate::combat_kit::PlayerKit;
use crate::combat_mode::CombatModeView;
use crate::combat_playback::{play_clip, CombatPlaybackState};
use crate::combat_profiles::CombatProfiles;
use crate::creature_anim::{ActionAnim, PlaySeq};
use crate::names::NameCache;
use crate::net::{ChatKind, ClientCommand, Guid, NetCommands, ObjectStore, SelfGuid, SelfPlayer};
use crate::player::Player;
use crate::query_cache::Ask;
use benilla_combat_core::ActionKind;

/// The addon prefix of every message on the link.
pub(crate) const PREFIX: &str = "RWSOULS";
/// `MODE 1` (and `HELLO` until answered) goes out again this often.
const KEEPALIVE_SECONDS: f32 = 10.0;
/// Reach slack for the creature's own size and the rendered position's lag, yards.
const STRIKE_SLACK_YARDS: f32 = 0.5;
/// A strike whose `RESOLVE` never comes is dropped this long past its windup.
const STRIKE_FORGET_SECONDS: f32 = 2.0;
/// A remote body's network swing stays muted this long after its last relayed `SWING`.
const REMOTE_MUTE_SECONDS: f32 = 3.0;
/// The inbox keeps at most this many unread messages.
const INBOX_CAP: usize = 256;
/// At most this many strikes are tracked at once.
const MAX_STRIKES: usize = 8;
/// `AnimationData` stand-ins for a relayed action on another player's body.
const ANIM_ATTACK_1H: u16 = 17;
const ANIM_PARRY_1H: u16 = 21;
const ANIM_DODGE: u16 = 30;

/// Inbound message bodies (the text after `RWSOULS\t`), filled by the chat handler.
static INBOX: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

/// Take an inbound addon text if it is the link's: `true` when it was (the chat log skips it).
pub(crate) fn tap_addon(text: &str) -> bool {
    let Some(body) = text
        .strip_prefix(PREFIX)
        .and_then(|rest| rest.strip_prefix('\t'))
    else {
        return false;
    };
    if let Ok(mut inbox) = INBOX.lock() {
        if inbox.len() >= INBOX_CAP {
            inbox.pop_front();
        }
        inbox.push_back(body.to_owned());
    }
    true
}

fn drain_inbox() -> Vec<String> {
    INBOX
        .lock()
        .map(|mut inbox| inbox.drain(..).collect())
        .unwrap_or_default()
}

/// Authored telegraph shapes. Reach in STRIKE is the base melee reach; both peers add the
/// thrust's two yards. Missing/unknown kinds retain the legacy 60-degree half-cone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StrikeKind {
    Legacy,
    Sweep,
    Thrust,
    Slam,
}

impl StrikeKind {
    fn parse(value: Option<&str>) -> Self {
        match value {
            Some("sweep") => Self::Sweep,
            Some("thrust") => Self::Thrust,
            Some("slam") => Self::Slam,
            _ => Self::Legacy,
        }
    }

    fn reach(self, base: f32) -> f32 {
        base + if self == Self::Thrust { 2.0 } else { 0.0 }
    }

    fn contains_angle(self, angle: f32) -> bool {
        let degrees: f32 = match self {
            Self::Legacy => 60.0,
            Self::Sweep => 90.0,
            Self::Thrust => 20.0,
            Self::Slam => return true,
        };
        angle.abs() <= degrees.to_radians()
    }
}

/// A server message.
#[derive(Clone, Debug, PartialEq)]
enum Inbound {
    Hello,
    Mode(bool),
    Peer {
        player: u64,
        sprint: bool,
    },
    Strike {
        id: u32,
        creature: u64,
        windup_ms: u32,
        reach: f32,
        mine: bool,
        kind: StrikeKind,
    },
    Resolve {
        id: u32,
        outcome: String,
    },
    Parried {
        creature: u64,
        riposte_ms: u32,
    },
    /// A creature's stance broke (ER's stance break): it staggers and the riposte window opens.
    Broken {
        creature: u64,
        riposte_ms: u32,
    },
    Swing {
        player: u64,
        action: String,
    },
    /// Our equipment load score, for the roll tier.
    Load(u8),
}

fn parse(body: &str) -> Option<Inbound> {
    let mut fields = body.split('\t');
    let verb = fields.next()?;
    let mut next = || fields.next();
    Some(match verb {
        "HELLO" => Inbound::Hello,
        "MODE" => Inbound::Mode(next()? == "1"),
        "PEER" => Inbound::Peer {
            player: next()?.parse().ok()?,
            sprint: next()? == "1",
        },
        "STRIKE" => Inbound::Strike {
            id: next()?.parse().ok()?,
            creature: next()?.parse().ok()?,
            windup_ms: next()?.parse().ok()?,
            reach: next()?.parse().ok()?,
            mine: next()? == "1",
            kind: StrikeKind::parse(next()),
        },
        "RESOLVE" => Inbound::Resolve {
            id: next()?.parse().ok()?,
            outcome: next()?.to_owned(),
        },
        "PARRIED" => Inbound::Parried {
            creature: next()?.parse().ok()?,
            riposte_ms: next()?.parse().ok()?,
        },
        "BROKEN" => Inbound::Broken {
            creature: next()?.parse().ok()?,
            riposte_ms: next()?.parse().ok()?,
        },
        "SWING" => Inbound::Swing {
            player: next()?.parse().ok()?,
            action: next()?.to_owned(),
        },
        "LOAD" => Inbound::Load(next()?.parse::<u32>().ok()?.min(255) as u8),
        _ => return None,
    })
}

/// A telegraphed enemy strike.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Strike {
    id: u32,
    creature: u64,
    /// App seconds: when the windup ends.
    deadline: f32,
    windup: f32,
    reach: f32,
    kind: StrikeKind,
    /// We judge it (we are its victim).
    mine: bool,
    /// The verdict we sent, once judged.
    verdict: Option<&'static str>,
}

/// A strike as the HUD draws it this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StrikeShown {
    /// The striking unit, whose overhead anchor seats the bar.
    pub(crate) unit: Entity,
    /// How far through the windup, `0..=1`.
    pub(crate) progress: f32,
    pub(crate) mine: bool,
    pub(crate) kind: StrikeKind,
}

/// The link's state. [`Default`] is "not announced, nothing pending".
#[derive(Resource, Default, Debug)]
pub(crate) struct SoulsLink {
    /// The server script answered.
    pub(crate) up: bool,
    /// When `MODE 1` last went out (app seconds); `None` while not announced.
    announced_at: Option<f32>,
    /// The sprint state the server was last told.
    sprint_sent: bool,
    /// The crouch state the server was last told.
    crouch_sent: bool,
    /// The action run last reported as `SWING`.
    swing_sent: Option<Instance>,
    /// Messages waiting to go out this frame.
    outbox: Vec<String>,
    strikes: Vec<Strike>,
    /// The strikes the HUD draws this frame.
    pub(crate) shown: Vec<StrikeShown>,
    /// A parried creature's guid and the end of its riposte window (app seconds).
    pub(crate) riposte: Option<(u64, f32)>,
    pub(super) broken: Option<(u64, f32)>,
    /// Other Souls players' bodies and when their last relayed `SWING` arrived.
    remote_swings: Vec<(Entity, f32)>,
    /// Creatures striking us and when their mute ends: the strike's own clip is the swing, so the
    /// damage report's stock swing (and its hit-key-delayed feedback) must not replay over it.
    striking: Vec<(Entity, f32)>,
    /// Our equipment load score as the server last sent it.
    pub(crate) load: Option<u8>,
}

impl SoulsLink {
    /// Whether hits go to the server instead of the stock melee fallback.
    pub(crate) fn reporting(&self) -> bool {
        self.up && self.announced_at.is_some()
    }

    /// Queue a hit window's struck units (nearest first) at `motion_value` thousandths, with the
    /// grip it was swung in ([`PlayerKit::two_handing`]).
    pub(crate) fn report_hit(&mut self, motion_value: i32, guids: &[u64], two_handing: bool) {
        if guids.is_empty() {
            return;
        }
        let list: Vec<String> = guids.iter().map(u64::to_string).collect();
        self.outbox.push(format!(
            "HIT\t{}\t{}\t{}",
            (motion_value.max(0) + 5) / 10,
            list.join(","),
            if two_handing { 2 } else { 1 }
        ));
    }

    pub(super) fn critical(&mut self, guid: u64, back: bool) {
        let suffix = if back { "\tback" } else { "" };
        self.outbox.push(format!("CRIT\t{guid}{suffix}"));
    }

    fn is_reset(&self) -> bool {
        !self.up
            && self.announced_at.is_none()
            && !self.sprint_sent
            && !self.crouch_sent
            && self.swing_sent.is_none()
            && self.outbox.is_empty()
            && self.strikes.is_empty()
            && self.shown.is_empty()
            && self.riposte.is_none()
            && self.broken.is_none()
            && self.remote_swings.is_empty()
            && self.striking.is_empty()
            && self.load.is_none()
    }
}

/// The bodies whose network swing animation is muted: ours while Souls combat is on (its swings
/// are the kit's clips, and the server's damage report would replay a stock swing over them), and
/// other Souls players' while their relayed actions play. Read by the animation driver.
#[derive(Resource, Default, Debug)]
pub(crate) struct SwingMute(pub(crate) Vec<Entity>);

impl SwingMute {
    pub(crate) fn muted(&self, entity: Entity) -> bool {
        self.0.contains(&entity)
    }
}

/// What a strike on us comes to, from where it lands and our state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Defence {
    parrying: bool,
    dodging: bool,
    guarding: bool,
}

fn verdict(in_reach: bool, in_cone: bool, defence: Defence) -> &'static str {
    if !in_reach || !in_cone {
        "miss"
    } else if defence.parrying {
        "parry"
    } else if defence.dodging {
        "dodge"
    } else if defence.guarding {
        "guard"
    } else {
        "hit"
    }
}

/// The anim a relayed action plays on another player's body.
fn relayed_anim(kit: Option<&benilla_combat_core::Kit>, action: &str) -> u16 {
    let row = super::retarget::relayed_row(kit, action);
    if action == "Parry" || row.is_some_and(|row| row.name == "Parry") {
        return ANIM_PARRY_1H;
    }
    let dodge = match row {
        Some(row) => row.kind == ActionKind::Dodge,
        None => action.contains("Roll") || action.starts_with("Backstep"),
    };
    if dodge {
        ANIM_DODGE
    } else {
        ANIM_ATTACK_1H
    }
}

/// Pick only authored sequences before allowing the generic fallback.
fn strike_anim(mut has: impl FnMut(u16) -> bool) -> u16 {
    [17, 18, 19, 16]
        .into_iter()
        .find(|&id| has(id))
        .unwrap_or(ANIM_ATTACK_1H)
}

/// The bodies the link looks up by guid.
type Bodies<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Guid,
        &'static ObjectStore,
        &'static GlobalTransform,
    ),
    Without<SelfPlayer>,
>;

/// Announce the mode, read the server, judge strikes, and send what is queued.
#[allow(clippy::too_many_arguments)]
pub(super) fn drive_link(
    time: Res<Time>,
    view: Res<CombatModeView>,
    config: Res<SoulsConfig>,
    kit: Res<PlayerKit>,
    profiles: Res<CombatProfiles>,
    playback: Res<CombatPlaybackState>,
    souls: Res<SoulsState>,
    player: Res<Player>,
    ids: (
        Res<SelfGuid>,
        Option<Res<NameCache>>,
        Option<Res<NetCommands>>,
        Commands,
    ),
    bodies: Bodies,
    models: Query<&benilla_assets::ModelAnimations>,
    me: Query<Entity, With<SelfPlayer>>,
    mut link: ResMut<SoulsLink>,
    mut mute: ResMut<SwingMute>,
    mut play_seq: ResMut<PlaySeq>,
    mut actions: MessageWriter<ActionAnim>,
) {
    let (self_guid, names, net, mut commands) = ids;
    let on = souls_on(&view) && config.features.server && crate::wrath::selected_from_env();
    let t = time.elapsed_secs();
    // Whom the message goes to: ourselves, by name.
    let own_name = self_guid.0.and_then(|guid| {
        names
            .as_deref()?
            .resolve_player(guid, Ask::Read)
            .map(str::to_owned)
    });
    let send = |text: String| {
        let (Some(net), Some(name)) = (net.as_deref(), own_name.as_ref()) else {
            return;
        };
        let command = ClientCommand::Chat {
            kind: ChatKind::Whisper,
            target: Some(name.clone()),
            text: format!("{PREFIX}\t{text}"),
            language: Some(u32::MAX),
        };
        if net.0.send(command).is_err() {
            debug!("souls link: not connected; {text:?} dropped");
        }
    };

    if !on {
        // Hand the server back its creatures and our run speed once, then forget everything.
        if link.announced_at.is_some() {
            send("MODE\t0".to_owned());
            info!("souls link: mode off sent");
        }
        let _ = drain_inbox();
        if !link.is_reset() {
            *link = SoulsLink::default();
        }
        let wanted: Vec<Entity> = Vec::new();
        if mute.0 != wanted {
            mute.0 = wanted;
        }
        return;
    }
    if own_name.is_none() {
        // Not in the world yet, or our name not cached: nothing can go out.
        let _ = drain_inbox();
        return;
    }

    // Announce, and keep announcing.
    let due = link
        .announced_at
        .is_none_or(|at| t - at >= KEEPALIVE_SECONDS);
    if due {
        if !link.up {
            send("HELLO".to_owned());
        }
        send("MODE\t1".to_owned());
        link.announced_at = Some(t);
    }

    let kit_now = current_kit(&kit, &profiles);
    // Read the server.
    for body in drain_inbox() {
        let Some(message) = parse(&body) else {
            debug!("souls link: unreadable {body:?}");
            continue;
        };
        match message {
            Inbound::Hello => {
                if !link.up {
                    info!("souls link: server script answered");
                }
                link.up = true;
            }
            Inbound::Mode(server_on) => {
                link.up = true;
                if !server_on {
                    // The server dropped us (a reload): announce again now.
                    link.announced_at = None;
                }
            }
            Inbound::Peer { player, sprint } => {
                if let Some((entity, ..)) = bodies.iter().find(|(_, guid, ..)| guid.0 == player) {
                    commands.entity(entity).insert(super::retarget::RemotePeer {
                        until: t + KEEPALIVE_SECONDS * 2.5,
                        sprint,
                    });
                }
            }
            Inbound::Strike {
                id,
                creature,
                windup_ms,
                reach,
                mine,
                kind,
            } => {
                if link.strikes.len() >= MAX_STRIKES {
                    link.strikes.remove(0);
                }
                let windup = windup_ms as f32 / 1000.0;
                if let Some((entity, ..)) = bodies.iter().find(|(_, guid, ..)| guid.0 == creature) {
                    let anim_id = strike_anim(|id| {
                        models
                            .get(entity)
                            .is_ok_and(|model| model.find(id).is_some())
                    });
                    actions.write(ActionAnim {
                        entity,
                        anim_id,
                        contact_in: Some(windup.max(0.001)),
                        seq: play_seq.next(),
                    });
                    let until = t + windup + STRIKE_FORGET_SECONDS;
                    match link.striking.iter_mut().find(|(e, _)| *e == entity) {
                        Some((_, at)) => *at = until,
                        None => link.striking.push((entity, until)),
                    }
                }
                link.strikes.push(Strike {
                    id,
                    creature,
                    deadline: t + windup,
                    windup,
                    reach,
                    kind,
                    // With the switch off we send no verdict; the server's area check decides.
                    mine: mine && config.features.enemy_strikes,
                    verdict: None,
                });
            }
            Inbound::Resolve { id, outcome } => {
                if let Some(at) = link.strikes.iter().position(|strike| strike.id == id) {
                    let strike = link.strikes.remove(at);
                    if strike.mine {
                        info!(
                            "souls: strike {id} from {:#x} resolved {outcome} (we said {:?})",
                            strike.creature, strike.verdict
                        );
                    }
                }
            }
            Inbound::Parried {
                creature,
                riposte_ms,
            } => {
                info!("souls: parried {creature:#x}; riposte open {riposte_ms} ms");
                link.riposte = Some((creature, t + riposte_ms as f32 / 1000.0));
            }
            Inbound::Broken {
                creature,
                riposte_ms,
            } => {
                if riposte_ms == 0 {
                    if link.broken.is_some_and(|(guid, _)| guid == creature) {
                        link.broken = None;
                    }
                } else {
                    link.broken = Some((creature, t + riposte_ms as f32 / 1000.0));
                }
            }
            Inbound::Swing {
                player: guid,
                action,
            } => {
                let Some((entity, ..)) = bodies.iter().find(|(_, g, ..)| g.0 == guid) else {
                    continue;
                };
                if let Some(row) = super::retarget::relayed_row(kit_now, &action) {
                    commands
                        .entity(entity)
                        .insert(super::retarget::RemoteAction {
                            source: row.source.clone(),
                            started: t,
                            duration: row.total.0 as f32
                                / 1000.0
                                / f32::from(kit_now.map_or(30, |kit| kit.frame_rate_hz).max(1)),
                        });
                } else {
                    commands
                        .entity(entity)
                        .remove::<super::retarget::RemoteAction>();
                }
                play_clip(
                    relayed_anim(kit_now, &action),
                    entity,
                    &mut play_seq,
                    &mut actions,
                );
                match link.remote_swings.iter_mut().find(|(e, _)| *e == entity) {
                    Some(entry) => entry.1 = t,
                    None => link.remote_swings.push((entity, t)),
                }
            }
            Inbound::Load(score) => {
                if link.load != Some(score) {
                    info!(
                        "souls link: equipment load {score}: {:?}",
                        crate::combat_kit::roll_load(Some(score))
                    );
                }
                link.load = Some(score);
            }
        }
    }
    if link.broken.is_some_and(|(_, until)| t >= until) {
        link.broken = None;
    }
    if link.riposte.is_some_and(|(_, until)| t >= until) {
        link.riposte = None;
    }

    // Judge our strikes at the end of their windup, and lay out the HUD's bars.
    let defence = Defence {
        parrying: config.features.parry
            && kit_now.is_some_and(|kit| in_parry_window(kit, &playback)),
        dodging: config.features.iframes && in_iframes(&playback, &profiles),
        guarding: config.features.guard && souls.guarding,
    };
    let mut verdicts = Vec::new();
    let mut shown = Vec::new();
    link.strikes
        .retain(|strike| t < strike.deadline + STRIKE_FORGET_SECONDS);
    for strike in &mut link.strikes {
        let body = bodies.iter().find(|(_, g, ..)| g.0 == strike.creature);
        if let Some((unit, ..)) = body {
            if strike.verdict.is_none() {
                let left = (strike.deadline - t).max(0.0);
                let progress = if strike.windup > 0.0 {
                    1.0 - left / strike.windup
                } else {
                    1.0
                };
                shown.push(StrikeShown {
                    unit,
                    progress: progress.clamp(0.0, 1.0),
                    mine: strike.mine,
                    kind: strike.kind,
                });
            }
        }
        if !strike.mine || strike.verdict.is_some() || t < strike.deadline {
            continue;
        }
        // A creature we cannot see is left to the server's area check.
        let Some((_, _, store, at)) = body else {
            strike.verdict = Some("unseen");
            continue;
        };
        let from = at.translation();
        let to_me = player.pos - from;
        let size = store.0.unit_combat_reach().clamp(0.0, 3.0);
        let in_reach = Vec2::new(to_me.x, to_me.z).length() - size
            <= strike.kind.reach(strike.reach) + STRIKE_SLACK_YARDS;
        let facing = at.compute_transform().rotation.to_euler(EulerRot::YXZ).0;
        let in_cone = strike.kind.contains_angle(off_facing(facing, to_me));
        let said = verdict(in_reach, in_cone, defence);
        strike.verdict = Some(said);
        verdicts.push(format!("VERDICT\t{}\t{said}", strike.id));
    }
    link.shown = shown;
    link.outbox.extend(verdicts);

    // Our own action's start, for the other Souls players to see.
    let running = kit_now.and_then(|kit| running_action(kit, &playback));
    let instance = running.map(|(instance, ..)| instance);
    if instance != link.swing_sent {
        if let Some((_, action, _)) = running {
            link.outbox.push(format!("SWING\t{}", action.name));
        }
        link.swing_sent = instance;
    }
    // The sprint.
    let sprint = souls.sprinting && config.features.sprint;
    if sprint != link.sprint_sent {
        link.outbox.push(format!("SPRINT\t{}", u8::from(sprint)));
        link.sprint_sent = sprint;
    }
    // The crouch: slower, and the crouch pose for other players.
    let crouch = souls.crouched && config.features.crouch;
    if crouch != link.crouch_sent {
        link.outbox.push(format!("CROUCH\t{}", u8::from(crouch)));
        link.crouch_sent = crouch;
    }

    for text in std::mem::take(&mut link.outbox) {
        send(text);
    }

    // The swing mute: ours while the server deals our damage (without it the fallback's stock
    // melee is the swing we see), and the remote bodies still playing a relayed action.
    link.remote_swings
        .retain(|(_, at)| t - *at < REMOTE_MUTE_SECONDS);
    let mut wanted: Vec<Entity> = if link.reporting() {
        me.iter().collect()
    } else {
        Vec::new()
    };
    wanted.extend(link.remote_swings.iter().map(|(entity, _)| *entity));
    link.striking.retain(|(_, until)| t < *until);
    wanted.extend(link.striking.iter().map(|(entity, _)| *entity));
    if mute.0 != wanted {
        mute.0 = wanted;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strike_animation_uses_the_first_authored_attack() {
        let preference = [17, 18, 19, 16];
        for start in 0..preference.len() {
            assert_eq!(
                strike_anim(|id| preference[start..].contains(&id)),
                preference[start]
            );
        }
        assert_eq!(strike_anim(|_| false), ANIM_ATTACK_1H);
    }

    #[test]
    fn server_messages_parse() {
        assert_eq!(parse("HELLO\t1"), Some(Inbound::Hello));
        assert_eq!(parse("MODE\t0"), Some(Inbound::Mode(false)));
        assert_eq!(
            parse("PEER\t42\t1"),
            Some(Inbound::Peer {
                player: 42,
                sprint: true
            })
        );
        assert_eq!(
            parse("STRIKE\t7\t17370386514018042881\t800\t5.33\t1"),
            Some(Inbound::Strike {
                id: 7,
                creature: 17_370_386_514_018_042_881,
                windup_ms: 800,
                reach: 5.33,
                mine: true,
                kind: StrikeKind::Legacy,
            })
        );
        assert_eq!(
            parse("PARRIED\t42\t3000"),
            Some(Inbound::Parried {
                creature: 42,
                riposte_ms: 3000
            })
        );
        assert_eq!(
            parse("BROKEN\t42\t3000"),
            Some(Inbound::Broken {
                creature: 42,
                riposte_ms: 3000
            })
        );
        assert_eq!(parse("STRIKE\t7\tnot-a-guid\t800\t5\t1"), None);
        assert_eq!(parse("WHAT\t1"), None);
    }

    /// Out of the swing is a miss whatever we do; inside it the parry beats the dodge beats the
    /// guard.
    #[test]
    fn verdict_order() {
        let none = Defence {
            parrying: false,
            dodging: false,
            guarding: false,
        };
        let all = Defence {
            parrying: true,
            dodging: true,
            guarding: true,
        };
        assert_eq!(verdict(false, true, all), "miss");
        assert_eq!(verdict(true, false, all), "miss");
        assert_eq!(verdict(true, true, all), "parry");
        assert_eq!(
            verdict(
                true,
                true,
                Defence {
                    parrying: false,
                    ..all
                }
            ),
            "dodge"
        );
        assert_eq!(
            verdict(
                true,
                true,
                Defence {
                    guarding: true,
                    ..none
                }
            ),
            "guard"
        );
        assert_eq!(verdict(true, true, none), "hit");
    }

    #[test]
    fn strike_shapes_parse_and_judge_their_own_geometry() {
        for (name, kind) in [
            ("sweep", StrikeKind::Sweep),
            ("thrust", StrikeKind::Thrust),
            ("slam", StrikeKind::Slam),
        ] {
            assert!(matches!(
                parse(&format!("STRIKE\t7\t42\t1200\t5\t1\t{name}")),
                Some(Inbound::Strike { kind: parsed, .. }) if parsed == kind
            ));
        }
        assert!(StrikeKind::Sweep.contains_angle(80f32.to_radians()));
        assert!(!StrikeKind::Sweep.contains_angle(100f32.to_radians()));
        assert_eq!(StrikeKind::Thrust.reach(5.0), 7.0);
        assert!(!StrikeKind::Thrust.contains_angle(30f32.to_radians()));
        assert!(StrikeKind::Slam.contains_angle(std::f32::consts::PI));
        assert_eq!(StrikeKind::Slam.reach(5.0), 5.0);
        assert_eq!(StrikeKind::parse(Some("future")), StrikeKind::Legacy);
    }

    #[test]
    fn hits_report_percent_and_guids() {
        let mut link = SoulsLink::default();
        link.report_hit(1_250, &[42, 7], false);
        link.report_hit(1_000, &[], true);
        link.report_hit(900, &[5], true);
        assert_eq!(
            link.outbox,
            vec!["HIT\t125\t42,7\t1".to_owned(), "HIT\t90\t5\t2".to_owned()]
        );
    }
}
