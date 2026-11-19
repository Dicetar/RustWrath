//! A weapon kit: the full per-action record the Souls player kit needs, and its local file.
//!
//! [`ActionSpec`] is the engine's compact view of an action: one window, one cancel offset, one
//! follow-up. The owner's decision to play the Elden Ring moveset as it is needs more than that —
//! a cancel offset per kind of input, a separate listening offset for the dodge, a charge window,
//! several hits with their own blade path, turn locks, and root motion. [`KitAction`] carries all
//! of it, in the donor's own units, and [`Kit::to_table`] compiles the part the engine consumes
//! today into an [`ActionTable`] at any [`TickRate`].
//!
//! **Units.** Times are animation frames at the kit's `frame_rate_hz`; distances are metres in
//! the character's own space; everything is fixed-point thousandths ([`Milli`]). The crate stays
//! free of floating point, a kit reads back exactly as it was written, and the frame-to-tick
//! conversion is one integer rule.
//!
//! **Where kits come from.** Nothing in this crate is game data. A kit is produced on the
//! owner's machine by [`crate::kit_import`] from data the owner generated from their own copy of
//! the game, and is stored in a local file that is never committed or shared. The text format
//! below is what that file holds.
//!
//! **What the engine consumes.** Total, the general and dodge listening offsets, the light,
//! heavy, dodge and guard cancel offsets, the first hit's window, a dodge's i-frames, the summed
//! stamina cost and the chain follow-up. Carried in the kit but **not** consumed by
//! [`ActionEngine`](crate::engine::ActionEngine) yet: the jump, movement and left-hand cancel
//! offsets, hits after the first, per-hit stamina timing, the charge window, turn locks and
//! rates, hit-stop, blade samples and root motion. Those wait for the collider, retarget and
//! movement slices; they are data here, not behaviour.
//!
//! # File format
//!
//! Line-oriented text. Blank lines and lines starting with `#` are ignored; leading and trailing
//! spaces are not significant. A time is a decimal number of frames, or `never`.
//!
//! ```text
//! rustwrath-kit 1
//! label <free text>
//! frame_rate <hz>
//! stamina_max <points>                       (optional)
//! action <id> <light|heavy|dodge>
//!   name <free text>
//!   source <clip id>
//!   total <frames>
//!   input <general> <dodge>                  (times)
//!   cancel <light> <heavy> <dodge> <jump> <guard> <move> <left>   (times)
//!   stamina <points>
//!   iframes <from> <to>                      (optional)
//!   airborne                                 (optional)
//!   charge <from> <to>                       (optional)
//!   next_light <id>                          (optional)
//!   next_heavy <id>                          (optional)
//!   hit <from> <to> <motion value> <guard damage> <stamina> <hit-stop s> <radius m>
//!   blade <x> <y> <z> <x> <y> <z>            (belongs to the hit above it)
//!   no_turn <from> <to>
//!   turn <from> <to> <degrees per second>
//!   motion <left> <up> <forward>             (one per frame, cumulative)
//! end
//! ```

use std::collections::BTreeSet;
use std::fmt;

use crate::action::{ActionId, ActionKind, ActionSpec, ActionTable, InputWindows};
use crate::identity::ActionRevision;
use crate::time::{TickRate, TickWindow};

/// The file format revision this crate reads and writes.
pub const KIT_FORMAT_VERSION: u32 = 1;

const MAGIC: &str = "rustwrath-kit";
const NEVER: &str = "never";

/// A fixed-point number in thousandths: frames, metres, seconds, points or degrees per second.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Milli(pub i32);

impl Milli {
    pub const ZERO: Self = Self(0);

    /// A whole number of units.
    pub const fn whole(units: i32) -> Self {
        Self(units.saturating_mul(1000))
    }

    /// Read a plain decimal literal (`12`, `-0.647`, `35.0`). Digits past the third decimal are
    /// rounded half away from zero. Exponents, signs other than a leading `-`, and anything out
    /// of range are refused.
    pub fn parse(text: &str) -> Option<Self> {
        let (negative, digits) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
        if whole.is_empty() && fraction.is_empty() {
            return None;
        }
        if !whole
            .bytes()
            .chain(fraction.bytes())
            .all(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        let mut value: i64 = 0;
        for byte in whole.bytes() {
            value = value.checked_mul(10)?.checked_add(i64::from(byte - b'0'))?;
            if value > i64::from(i32::MAX) {
                return None;
            }
        }
        value *= 1000;
        for (place, byte) in fraction.bytes().enumerate().take(4) {
            let digit = i64::from(byte - b'0');
            match place {
                0 => value += digit * 100,
                1 => value += digit * 10,
                2 => value += digit,
                _ => value += i64::from(digit >= 5),
            }
        }
        i32::try_from(if negative { -value } else { value })
            .ok()
            .map(Self)
    }
}

impl fmt::Display for Milli {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let magnitude = self.0.unsigned_abs();
        write!(f, "{sign}{}.{:03}", magnitude / 1000, magnitude % 1000)
    }
}

/// Why a kit could not be read, checked or compiled.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct KitError {
    /// One-based line of the text being read; zero when the failure is not tied to a line.
    pub line: usize,
    pub message: String,
}

impl KitError {
    pub(crate) fn at(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
        }
    }

    pub(crate) fn general(message: impl Into<String>) -> Self {
        Self::at(0, message)
    }
}

impl fmt::Display for KitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            0 => write!(f, "{}", self.message),
            line => write!(f, "line {line}: {}", self.message),
        }
    }
}

impl std::error::Error for KitError {}

/// Earliest frame each kind of queued input may replace the action; `None` is never.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct KitCancels {
    pub light: Option<Milli>,
    pub heavy: Option<Milli>,
    pub dodge: Option<Milli>,
    pub jump: Option<Milli>,
    pub guard: Option<Milli>,
    /// From when movement input ends the recovery.
    pub movement: Option<Milli>,
    /// From when a left-hand attack can take over.
    pub left: Option<Milli>,
}

/// One hit of an attack.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct KitHit {
    /// Active window in frames, `from <= frame < to`.
    pub from: Milli,
    pub to: Milli,
    /// Multiplier on attack rating.
    pub motion_value: Milli,
    /// Multiplier on stamina damage dealt to a guarding target.
    pub guard_damage: Milli,
    /// Stamina taken when this hit comes out.
    pub stamina: Milli,
    /// Seconds attacker and target both freeze for when it lands.
    pub hit_stop: Milli,
    /// Thickness of the hit capsule around the blade, metres.
    pub radius: Milli,
    /// The blade's two ends on each frame from `from` (rounded down) to `to` (rounded up), as
    /// `[x, y, z, x, y, z]` in the character's own space.
    pub blade: Vec<[Milli; 6]>,
}

/// One action of a kit, with everything the donor data says about it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct KitAction {
    /// The id this action has in a table compiled from the kit.
    pub id: ActionId,
    pub name: String,
    /// The animation clip this was read from.
    pub source: String,
    /// [`ActionKind::MeleeLight`], [`ActionKind::MeleeHeavy`] or [`ActionKind::Dodge`].
    pub kind: ActionKind,
    /// Frame the animation ends and control returns on its own.
    pub total: Milli,
    /// Presses before this frame are ignored rather than queued; `None` is never heard.
    pub input_from: Option<Milli>,
    /// The same for a dodge press.
    pub dodge_input_from: Option<Milli>,
    pub cancel: KitCancels,
    /// Invincibility window in frames, `from <= frame < to`.
    pub iframes: Option<(Milli, Milli)>,
    /// Low attacks pass underneath while this is airborne.
    pub airborne: bool,
    /// A dodge's cost. For an attack, the first hit's cost; each hit carries its own.
    pub stamina: Milli,
    /// Every hit of the attack, in order.
    pub hits: Vec<KitHit>,
    /// Window in which releasing the button swaps to the uncharged attack.
    pub charge: Option<(Milli, Milli)>,
    /// Windows in which the character cannot be turned.
    pub no_turn: Vec<(Milli, Milli)>,
    /// `(from, to, degrees per second)` turn-rate overrides.
    pub turn: Vec<(Milli, Milli, Milli)>,
    /// Root motion, one sample per frame: cumulative `[left, up, forward]` metres.
    pub motion: Vec<[Milli; 3]>,
    /// What a light press continues into from this action.
    pub next_light: Option<ActionId>,
    /// What a heavy press continues into from this action.
    pub next_heavy: Option<ActionId>,
}

/// A set of actions for one weapon and grip.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Kit {
    pub label: String,
    /// The animation frame rate every time in the kit is counted at.
    pub frame_rate_hz: u16,
    /// The stamina bar the costs were measured against, when the source states one.
    pub stamina_max: Option<Milli>,
    pub actions: Vec<KitAction>,
}

/// Frames to ticks, rounded to the nearest tick (halves up). The rule is monotonic, so the
/// order of offsets inside an action survives the conversion.
fn frames_to_ticks(frames: Milli, frame_rate_hz: u16, rate: TickRate) -> u64 {
    let numerator = u64::from(frames.0.max(0).unsigned_abs()) * u64::from(rate.hz());
    let denominator = 1000 * u64::from(frame_rate_hz.max(1));
    (2 * numerator + denominator) / (2 * denominator)
}

fn kind_word(kind: ActionKind) -> Option<&'static str> {
    match kind {
        ActionKind::MeleeLight => Some("light"),
        ActionKind::MeleeHeavy => Some("heavy"),
        ActionKind::Dodge => Some("dodge"),
        _ => None,
    }
}

fn kind_from_word(word: &str) -> Option<ActionKind> {
    match word {
        "light" => Some(ActionKind::MeleeLight),
        "heavy" => Some(ActionKind::MeleeHeavy),
        "dodge" => Some(ActionKind::Dodge),
        _ => None,
    }
}

impl KitAction {
    /// Cumulative `[left, up, forward]` root motion at `frame`, interpolated between samples and
    /// held at the ends.
    pub fn motion_at(&self, frame: Milli) -> [Milli; 3] {
        let Some(last) = self.motion.len().checked_sub(1) else {
            return [Milli::ZERO; 3];
        };
        let position = frame.0.max(0).unsigned_abs() as usize;
        let index = (position / 1000).min(last);
        let next = (index + 1).min(last);
        let fraction = (position % 1000) as i64;
        let (a, b) = (self.motion[index], self.motion[next]);
        std::array::from_fn(|axis| {
            let (a, b) = (i64::from(a[axis].0), i64::from(b[axis].0));
            Milli((a + ((b - a) * fraction).div_euclid(1000)) as i32)
        })
    }

    /// Compile the part of this action the engine consumes, in ticks at `rate`.
    ///
    /// - Offsets round to the nearest tick. An offset past the action's end means the action
    ///   finishes first, so it compiles to never.
    /// - A window is clamped to the action and kept at least one tick long.
    /// - An attack's contact window is its first hit; its cost is the sum of its hits, charged at
    ///   the start (the engine has no per-hit spend yet).
    /// - `cancel_from_ticks`, the offset for request kinds without a class of their own, is the
    ///   latest of the light, heavy, dodge and guard offsets: the point from which anything may
    ///   take over.
    /// - Interruption is project policy, not donor data: an attack yields up to its contact
    ///   start, a dodge for its whole length — the rules the profile compiler already uses.
    pub fn compile(
        &self,
        frame_rate_hz: u16,
        rate: TickRate,
    ) -> Result<(ActionSpec, InputWindows), KitError> {
        let fail =
            |message: String| KitError::general(format!("action `{}`: {message}", self.name));
        let to_ticks = |frames: Milli| frames_to_ticks(frames, frame_rate_hz, rate);
        let total = to_ticks(self.total);
        if total == 0 {
            return Err(fail("is shorter than one tick at this rate".into()));
        }
        let offset = |frames: Option<Milli>| frames.map(to_ticks).filter(|&ticks| ticks <= total);
        let window = |from: Milli, to: Milli| {
            let from = to_ticks(from);
            let to = to_ticks(to).min(total).max(from + 1);
            if to > total {
                return Err(fail(format!(
                    "a window starting at tick {from} does not fit in {total} ticks"
                )));
            }
            Ok(TickWindow { from, to })
        };

        let (iframes, contact, cost, interruptible_until) = match self.kind {
            ActionKind::Dodge => {
                let (from, to) = self
                    .iframes
                    .ok_or_else(|| fail("a dodge needs i-frames".into()))?;
                (
                    Some(window(from, to)?),
                    None,
                    i64::from(self.stamina.0),
                    Some(total),
                )
            }
            ActionKind::MeleeLight | ActionKind::MeleeHeavy => {
                let first = self
                    .hits
                    .first()
                    .ok_or_else(|| fail("an attack needs a hit".into()))?;
                let contact = window(first.from, first.to)?;
                let cost = self
                    .hits
                    .iter()
                    .map(|hit| i64::from(hit.stamina.0))
                    .sum::<i64>();
                (None, Some(contact), cost, Some(contact.from))
            }
            _ => return Err(fail("only light, heavy and dodge actions compile".into())),
        };
        let stamina_cost = u32::try_from((cost.max(0) + 500) / 1000)
            .map_err(|_| fail("stamina cost is out of range".into()))?;

        let inputs = InputWindows {
            dodge_input_from: offset(self.dodge_input_from),
            cancel_light: offset(self.cancel.light),
            cancel_heavy: offset(self.cancel.heavy),
            cancel_dodge: offset(self.cancel.dodge),
            cancel_guard: offset(self.cancel.guard),
        };
        let cancel_from_ticks = [
            inputs.cancel_light,
            inputs.cancel_heavy,
            inputs.cancel_dodge,
            inputs.cancel_guard,
        ]
        .into_iter()
        .flatten()
        .max();
        let spec = ActionSpec {
            id: self.id,
            kind: self.kind,
            total_ticks: total,
            input_from_ticks: offset(self.input_from).unwrap_or(total),
            cancel_from_ticks,
            iframes,
            contact,
            stamina_cost,
            interruptible_until,
            // The follow-up gates requests of its own kind: a heavy attack names the next heavy,
            // everything else the swing a light press continues into.
            followup: match self.kind {
                ActionKind::MeleeHeavy => self.next_heavy,
                _ => self.next_light,
            },
        };
        spec.validate().map_err(|error| fail(error.to_string()))?;
        Ok((spec, inputs))
    }
}

impl Kit {
    /// The frame a running action is on after `elapsed_ticks` at `rate`.
    pub fn frame_at_tick(&self, rate: TickRate, elapsed_ticks: u64) -> Milli {
        let frames = elapsed_ticks.saturating_mul(1000 * u64::from(self.frame_rate_hz))
            / u64::from(rate.hz());
        Milli(i32::try_from(frames).unwrap_or(i32::MAX))
    }

    /// The action with this id.
    pub fn action(&self, id: ActionId) -> Option<&KitAction> {
        self.actions.iter().find(|action| action.id == id)
    }

    /// The action with this name. Names, not ids, are what stays the same between kits.
    pub fn action_named(&self, name: &str) -> Option<&KitAction> {
        self.actions.iter().find(|action| action.name == name)
    }

    /// Check the kit is complete and self-consistent. Reading a file and compiling a table both
    /// run this.
    pub fn validate(&self) -> Result<(), KitError> {
        if self.frame_rate_hz == 0 {
            return Err(KitError::general("frame_rate must be non-zero"));
        }
        if self.label.chars().any(char::is_control) {
            return Err(KitError::general("label must be one line"));
        }
        if self.actions.is_empty() {
            return Err(KitError::general("a kit needs at least one action"));
        }
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        for action in &self.actions {
            let fail =
                |message: &str| KitError::general(format!("action `{}`: {message}", action.name));
            if action.id.0 == 0 {
                return Err(fail("id 0 is reserved"));
            }
            if !ids.insert(action.id) {
                return Err(fail("its id is used twice"));
            }
            let name = action.name.as_str();
            if name.is_empty() || name != name.trim() || name.chars().any(char::is_control) {
                return Err(fail("name must be one non-empty line without outer spaces"));
            }
            if !names.insert(name) {
                return Err(fail("its name is used twice"));
            }
            if action.source.is_empty()
                || action
                    .source
                    .chars()
                    .any(|c| c.is_whitespace() || c.is_control())
            {
                return Err(fail("source must be one word"));
            }
            if kind_word(action.kind).is_none() {
                return Err(fail("kind must be light, heavy or dodge"));
            }
            if action.total.0 <= 0 {
                return Err(fail("total must be positive"));
            }
            let cancel = action.cancel;
            let times = [
                action.input_from,
                action.dodge_input_from,
                cancel.light,
                cancel.heavy,
                cancel.dodge,
                cancel.jump,
                cancel.guard,
                cancel.movement,
                cancel.left,
            ];
            if times.iter().flatten().any(|time| time.0 < 0) || action.stamina.0 < 0 {
                return Err(fail("times and costs cannot be negative"));
            }
            let windows = action
                .iframes
                .into_iter()
                .chain(action.charge)
                .chain(action.no_turn.iter().copied())
                .chain(action.turn.iter().map(|&(from, to, _)| (from, to)))
                .chain(action.hits.iter().map(|hit| (hit.from, hit.to)));
            for (from, to) in windows {
                if from.0 < 0 || from >= to {
                    return Err(fail(
                        "a window must start at or after zero and end after it starts",
                    ));
                }
            }
            match action.kind {
                ActionKind::Dodge => {
                    if action.iframes.is_none() || !action.hits.is_empty() {
                        return Err(fail("a dodge needs i-frames and no hits"));
                    }
                }
                _ => {
                    if action.hits.is_empty() {
                        return Err(fail("an attack needs at least one hit"));
                    }
                }
            }
        }
        for action in &self.actions {
            for next in [action.next_light, action.next_heavy].into_iter().flatten() {
                if next == action.id || !ids.contains(&next) {
                    return Err(KitError::general(format!(
                        "action `{}`: continues into {}, which is itself or missing",
                        action.name, next.0
                    )));
                }
            }
        }
        Ok(())
    }

    /// Compile every action into an engine table at `rate`, with per-input timing attached.
    pub fn to_table(
        &self,
        revision: ActionRevision,
        rate: TickRate,
    ) -> Result<ActionTable, KitError> {
        self.validate()?;
        let mut table = ActionTable::new(revision, rate);
        for action in &self.actions {
            let (spec, inputs) = action.compile(self.frame_rate_hz, rate)?;
            table
                .insert_with_inputs(spec, inputs)
                .map_err(|error| KitError::general(format!("action `{}`: {error}", action.name)))?;
        }
        table
            .validate()
            .map_err(|error| KitError::general(error.to_string()))?;
        Ok(table)
    }

    /// Write the kit in the text format described in the module documentation.
    pub fn to_text(&self) -> String {
        use fmt::Write as _;

        fn time(value: Option<Milli>) -> String {
            value.map_or_else(|| NEVER.to_string(), |value| value.to_string())
        }
        fn row(values: &[Milli]) -> String {
            values
                .iter()
                .map(Milli::to_string)
                .collect::<Vec<_>>()
                .join(" ")
        }

        let mut out = String::new();
        // Writing to a String cannot fail.
        let _ = writeln!(out, "{MAGIC} {KIT_FORMAT_VERSION}");
        out.push_str("# Generated on this machine from the owner's own game data.\n");
        out.push_str("# Local file: never commit or share it.\n");
        let _ = writeln!(out, "label {}", self.label);
        let _ = writeln!(out, "frame_rate {}", self.frame_rate_hz);
        if let Some(stamina_max) = self.stamina_max {
            let _ = writeln!(out, "stamina_max {stamina_max}");
        }
        for action in &self.actions {
            let cancel = action.cancel;
            let _ = writeln!(
                out,
                "action {} {}",
                action.id.0,
                kind_word(action.kind).unwrap_or("unsupported")
            );
            let _ = writeln!(out, "  name {}", action.name);
            let _ = writeln!(out, "  source {}", action.source);
            let _ = writeln!(out, "  total {}", action.total);
            let _ = writeln!(
                out,
                "  input {} {}",
                time(action.input_from),
                time(action.dodge_input_from)
            );
            let _ = writeln!(
                out,
                "  cancel {} {} {} {} {} {} {}",
                time(cancel.light),
                time(cancel.heavy),
                time(cancel.dodge),
                time(cancel.jump),
                time(cancel.guard),
                time(cancel.movement),
                time(cancel.left)
            );
            let _ = writeln!(out, "  stamina {}", action.stamina);
            if let Some((from, to)) = action.iframes {
                let _ = writeln!(out, "  iframes {from} {to}");
            }
            if action.airborne {
                out.push_str("  airborne\n");
            }
            if let Some((from, to)) = action.charge {
                let _ = writeln!(out, "  charge {from} {to}");
            }
            if let Some(next) = action.next_light {
                let _ = writeln!(out, "  next_light {}", next.0);
            }
            if let Some(next) = action.next_heavy {
                let _ = writeln!(out, "  next_heavy {}", next.0);
            }
            for hit in &action.hits {
                let _ = writeln!(
                    out,
                    "  hit {}",
                    row(&[
                        hit.from,
                        hit.to,
                        hit.motion_value,
                        hit.guard_damage,
                        hit.stamina,
                        hit.hit_stop,
                        hit.radius
                    ])
                );
                for sample in &hit.blade {
                    let _ = writeln!(out, "  blade {}", row(sample));
                }
            }
            for &(from, to) in &action.no_turn {
                let _ = writeln!(out, "  no_turn {from} {to}");
            }
            for &(from, to, degrees) in &action.turn {
                let _ = writeln!(out, "  turn {from} {to} {degrees}");
            }
            for sample in &action.motion {
                let _ = writeln!(out, "  motion {}", row(sample));
            }
            out.push_str("end\n");
        }
        out
    }

    /// Read a kit from its text format and validate it.
    pub fn parse(text: &str) -> Result<Self, KitError> {
        let mut header_seen = false;
        let mut label = None;
        let mut frame_rate_hz = None;
        let mut stamina_max = None;
        let mut actions = Vec::new();
        let mut draft: Option<Draft> = None;

        for (index, raw) in text.lines().enumerate() {
            let line = index + 1;
            let content = raw.trim();
            if content.is_empty() || content.starts_with('#') {
                continue;
            }
            let (keyword, rest) = match content.split_once(char::is_whitespace) {
                Some((keyword, rest)) => (keyword, rest.trim()),
                None => (content, ""),
            };
            if !header_seen {
                if keyword != MAGIC {
                    return Err(KitError::at(
                        line,
                        format!("not a kit file: expected `{MAGIC} {KIT_FORMAT_VERSION}`"),
                    ));
                }
                if rest.parse::<u32>().ok() != Some(KIT_FORMAT_VERSION) {
                    return Err(KitError::at(
                        line,
                        format!("kit format `{rest}` is not supported; this build reads version {KIT_FORMAT_VERSION}"),
                    ));
                }
                header_seen = true;
                continue;
            }
            let Some(action) = draft.as_mut() else {
                match keyword {
                    "label" => set_once(&mut label, rest.to_string(), line, keyword)?,
                    "frame_rate" => {
                        let hz = rest.parse::<u16>().map_err(|_| {
                            KitError::at(line, "frame_rate must be a whole number of hertz")
                        })?;
                        set_once(&mut frame_rate_hz, hz, line, keyword)?;
                    }
                    "stamina_max" => {
                        let [points] = numbers(rest, line)?;
                        set_once(&mut stamina_max, points, line, keyword)?;
                    }
                    "action" => {
                        let mut words = rest.split_whitespace();
                        let id = words
                            .next()
                            .and_then(|word| word.parse::<u32>().ok())
                            .ok_or_else(|| KitError::at(line, "action needs a numeric id"))?;
                        let kind = words.next().and_then(kind_from_word).ok_or_else(|| {
                            KitError::at(line, "action kind must be light, heavy or dodge")
                        })?;
                        if words.next().is_some() {
                            return Err(KitError::at(line, "action takes an id and a kind"));
                        }
                        draft = Some(Draft::new(ActionId(id), kind, line));
                    }
                    other => {
                        return Err(KitError::at(
                            line,
                            format!("unknown keyword `{other}` outside an action"),
                        ))
                    }
                }
                continue;
            };
            match keyword {
                "name" => set_once(&mut action.name, rest.to_string(), line, keyword)?,
                "source" => set_once(&mut action.source, rest.to_string(), line, keyword)?,
                "total" => {
                    let [total] = numbers(rest, line)?;
                    set_once(&mut action.total, total, line, keyword)?;
                }
                "input" => {
                    let [general, dodge] = times(rest, line)?;
                    set_once(&mut action.input, (general, dodge), line, keyword)?;
                }
                "cancel" => {
                    let [light, heavy, dodge, jump, guard, movement, left] = times(rest, line)?;
                    let cancel = KitCancels {
                        light,
                        heavy,
                        dodge,
                        jump,
                        guard,
                        movement,
                        left,
                    };
                    set_once(&mut action.cancel, cancel, line, keyword)?;
                }
                "stamina" => {
                    let [stamina] = numbers(rest, line)?;
                    set_once(&mut action.stamina, stamina, line, keyword)?;
                }
                "iframes" => {
                    let [from, to] = numbers(rest, line)?;
                    set_once(&mut action.iframes, (from, to), line, keyword)?;
                }
                "airborne" => {
                    if !rest.is_empty() || action.airborne {
                        return Err(KitError::at(
                            line,
                            "airborne takes no value and appears once",
                        ));
                    }
                    action.airborne = true;
                }
                "charge" => {
                    let [from, to] = numbers(rest, line)?;
                    set_once(&mut action.charge, (from, to), line, keyword)?;
                }
                "next_light" => set_once(
                    &mut action.next_light,
                    action_ref(rest, line)?,
                    line,
                    keyword,
                )?,
                "next_heavy" => set_once(
                    &mut action.next_heavy,
                    action_ref(rest, line)?,
                    line,
                    keyword,
                )?,
                "hit" => {
                    let [from, to, motion_value, guard_damage, stamina, hit_stop, radius] =
                        numbers(rest, line)?;
                    action.hits.push(KitHit {
                        from,
                        to,
                        motion_value,
                        guard_damage,
                        stamina,
                        hit_stop,
                        radius,
                        blade: Vec::new(),
                    });
                }
                "blade" => {
                    let sample = numbers(rest, line)?;
                    action
                        .hits
                        .last_mut()
                        .ok_or_else(|| KitError::at(line, "blade must follow a hit"))?
                        .blade
                        .push(sample);
                }
                "no_turn" => {
                    let [from, to] = numbers(rest, line)?;
                    action.no_turn.push((from, to));
                }
                "turn" => {
                    let [from, to, degrees] = numbers(rest, line)?;
                    action.turn.push((from, to, degrees));
                }
                "motion" => action.motion.push(numbers(rest, line)?),
                "end" => {
                    if !rest.is_empty() {
                        return Err(KitError::at(line, "end takes no value"));
                    }
                    if let Some(done) = draft.take() {
                        actions.push(done.finish()?);
                    }
                }
                other => {
                    return Err(KitError::at(
                        line,
                        format!("unknown keyword `{other}` in an action"),
                    ))
                }
            }
        }

        if !header_seen {
            return Err(KitError::general(format!(
                "not a kit file: expected `{MAGIC} {KIT_FORMAT_VERSION}`"
            )));
        }
        if let Some(open) = draft {
            return Err(KitError::at(open.line, "this action has no `end`"));
        }
        let kit = Self {
            label: label.ok_or_else(|| KitError::general("missing `label`"))?,
            frame_rate_hz: frame_rate_hz
                .ok_or_else(|| KitError::general("missing `frame_rate`"))?,
            stamina_max,
            actions,
        };
        kit.validate()?;
        Ok(kit)
    }
}

/// An action being read: single-valued lines are `None` until seen.
struct Draft {
    id: ActionId,
    kind: ActionKind,
    line: usize,
    name: Option<String>,
    source: Option<String>,
    total: Option<Milli>,
    input: Option<(Option<Milli>, Option<Milli>)>,
    cancel: Option<KitCancels>,
    stamina: Option<Milli>,
    iframes: Option<(Milli, Milli)>,
    airborne: bool,
    charge: Option<(Milli, Milli)>,
    next_light: Option<ActionId>,
    next_heavy: Option<ActionId>,
    hits: Vec<KitHit>,
    no_turn: Vec<(Milli, Milli)>,
    turn: Vec<(Milli, Milli, Milli)>,
    motion: Vec<[Milli; 3]>,
}

impl Draft {
    fn new(id: ActionId, kind: ActionKind, line: usize) -> Self {
        Self {
            id,
            kind,
            line,
            name: None,
            source: None,
            total: None,
            input: None,
            cancel: None,
            stamina: None,
            iframes: None,
            airborne: false,
            charge: None,
            next_light: None,
            next_heavy: None,
            hits: Vec::new(),
            no_turn: Vec::new(),
            turn: Vec::new(),
            motion: Vec::new(),
        }
    }

    fn finish(self) -> Result<KitAction, KitError> {
        let line = self.line;
        let missing = |what: &str| KitError::at(line, format!("this action has no `{what}`"));
        let (input_from, dodge_input_from) = self.input.ok_or_else(|| missing("input"))?;
        Ok(KitAction {
            id: self.id,
            name: self.name.ok_or_else(|| missing("name"))?,
            source: self.source.ok_or_else(|| missing("source"))?,
            kind: self.kind,
            total: self.total.ok_or_else(|| missing("total"))?,
            input_from,
            dodge_input_from,
            cancel: self.cancel.ok_or_else(|| missing("cancel"))?,
            iframes: self.iframes,
            airborne: self.airborne,
            stamina: self.stamina.ok_or_else(|| missing("stamina"))?,
            hits: self.hits,
            charge: self.charge,
            no_turn: self.no_turn,
            turn: self.turn,
            motion: self.motion,
            next_light: self.next_light,
            next_heavy: self.next_heavy,
        })
    }
}

fn set_once<T>(slot: &mut Option<T>, value: T, line: usize, keyword: &str) -> Result<(), KitError> {
    if slot.is_some() {
        return Err(KitError::at(line, format!("`{keyword}` appears twice")));
    }
    *slot = Some(value);
    Ok(())
}

fn fields<const N: usize, T: Copy + Default>(
    rest: &str,
    line: usize,
    read: impl Fn(&str) -> Option<T>,
) -> Result<[T; N], KitError> {
    let mut values = [T::default(); N];
    let mut words = rest.split_whitespace();
    for value in &mut values {
        let word = words
            .next()
            .ok_or_else(|| KitError::at(line, format!("expected {N} values")))?;
        *value = read(word)
            .ok_or_else(|| KitError::at(line, format!("`{word}` is not a valid value here")))?;
    }
    if words.next().is_some() {
        return Err(KitError::at(line, format!("expected {N} values")));
    }
    Ok(values)
}

fn numbers<const N: usize>(rest: &str, line: usize) -> Result<[Milli; N], KitError> {
    fields(rest, line, Milli::parse)
}

fn times<const N: usize>(rest: &str, line: usize) -> Result<[Option<Milli>; N], KitError> {
    fields(rest, line, |word| match word {
        NEVER => Some(None),
        number => Milli::parse(number).map(Some),
    })
}

fn action_ref(rest: &str, line: usize) -> Result<ActionId, KitError> {
    rest.parse::<u32>()
        .map(ActionId)
        .map_err(|_| KitError::at(line, "expected an action id"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(text: &str) -> Milli {
        Milli::parse(text).expect("literal")
    }

    /// Authored values only: nothing here is taken from any game.
    fn swing(id: u32, name: &str) -> KitAction {
        KitAction {
            id: ActionId(id),
            name: name.to_string(),
            source: format!("clip_{id}"),
            kind: ActionKind::MeleeLight,
            total: m("30"),
            input_from: Some(m("3")),
            dodge_input_from: Some(m("1.5")),
            cancel: KitCancels {
                light: Some(m("12")),
                heavy: Some(m("15")),
                dodge: Some(m("9")),
                jump: None,
                guard: Some(m("18")),
                movement: Some(m("21")),
                left: None,
            },
            iframes: None,
            airborne: false,
            stamina: m("7"),
            hits: vec![
                KitHit {
                    from: m("6"),
                    to: m("9"),
                    motion_value: m("1"),
                    guard_damage: m("0.5"),
                    stamina: m("7"),
                    hit_stop: m("0.05"),
                    radius: m("0.125"),
                    blade: vec![[m("-0.5"), m("1"), m("0.25"), m("-1.5"), m("1.125"), m("0")]],
                },
                KitHit {
                    from: m("15"),
                    to: m("16.5"),
                    motion_value: m("0.75"),
                    guard_damage: m("0.5"),
                    stamina: m("4.4"),
                    hit_stop: m("0.05"),
                    radius: m("0.125"),
                    blade: Vec::new(),
                },
            ],
            charge: Some((m("3"), m("5"))),
            no_turn: vec![(m("4"), m("33"))],
            turn: vec![(m("0"), m("4"), m("540"))],
            motion: vec![
                [m("0"), m("0"), m("0")],
                [m("0.1"), m("0"), m("0.5")],
                [m("0.1"), m("0"), m("1.5")],
            ],
            next_light: None,
            next_heavy: None,
        }
    }

    fn roll(id: u32) -> KitAction {
        KitAction {
            id: ActionId(id),
            name: "Roll (authored, forward)".to_string(),
            source: "clip_roll".to_string(),
            kind: ActionKind::Dodge,
            total: m("24"),
            input_from: Some(m("6")),
            dodge_input_from: Some(m("6")),
            cancel: KitCancels {
                light: Some(m("18")),
                heavy: Some(m("18")),
                dodge: Some(m("21")),
                jump: None,
                guard: None,
                movement: Some(m("22")),
                left: None,
            },
            iframes: Some((m("0"), m("9"))),
            airborne: true,
            stamina: m("10"),
            hits: Vec::new(),
            charge: None,
            no_turn: Vec::new(),
            turn: Vec::new(),
            motion: Vec::new(),
            next_light: None,
            next_heavy: None,
        }
    }

    fn kit() -> Kit {
        let mut first = swing(1, "Swing one");
        first.next_light = Some(ActionId(2));
        let mut second = swing(2, "Swing two");
        second.next_light = Some(ActionId(1));
        Kit {
            label: "Authored test blade, one-handed".to_string(),
            frame_rate_hz: 30,
            stamina_max: Some(m("80")),
            actions: vec![first, second, roll(3)],
        }
    }

    #[test]
    fn decimals_read_and_write_exactly() {
        assert_eq!(m("-0.647"), Milli(-647));
        assert_eq!(m("35.0"), Milli(35_000));
        assert_eq!(m("-0.000"), Milli::ZERO);
        assert_eq!(m("12"), Milli::whole(12));
        assert_eq!(
            m("0.0005"),
            Milli(1),
            "the fourth decimal rounds half away from zero"
        );
        assert_eq!(m("-1.2344"), Milli(-1234));
        assert_eq!(Milli(-647).to_string(), "-0.647");
        assert_eq!(Milli(35_000).to_string(), "35.000");
        for refused in ["", "-", ".", "1e3", "+1", "1.2.3", "0x10", "3000000"] {
            assert_eq!(Milli::parse(refused), None, "{refused:?}");
        }
    }

    #[test]
    fn a_kit_survives_its_text_form() {
        let kit = kit();
        let text = kit.to_text();
        assert!(text.starts_with("rustwrath-kit 1\n"));
        assert_eq!(Kit::parse(&text).expect("parse"), kit);
        // Comments, blank lines and indentation are not significant.
        let noisy = text
            .replace("  ", "\t ")
            .replace("end\n", "end\n\n# next\n");
        assert_eq!(Kit::parse(&noisy).expect("parse"), kit);
    }

    #[test]
    fn a_bad_file_is_refused_with_its_line() {
        let text = kit().to_text();
        let cases = [
            (
                text.replacen("rustwrath-kit 1", "rustwrath-kit 2", 1),
                1,
                "not supported",
            ),
            (
                text.replacen("  total 30.000", "  total thirty", 1),
                10,
                "not a valid value",
            ),
            (
                text.replacen("  total 30.000", "  wobble 1", 1),
                10,
                "unknown keyword",
            ),
            (text.replacen("  total 30.000\n", "", 1), 7, "no `total`"),
            (
                text.replacen("  stamina 7.000", "  stamina 7.000\n  stamina 7.000", 1),
                14,
                "appears twice",
            ),
            (
                text.replacen("  input 3.000 1.500", "  input 3.000", 1),
                11,
                "expected 2 values",
            ),
        ];
        for (broken, line, needle) in cases {
            let error = Kit::parse(&broken).expect_err(needle);
            assert_eq!(error.line, line, "{error}");
            assert!(error.message.contains(needle), "{error}");
        }
        let unfinished = text.trim_end().strip_suffix("end").expect("ends with end");
        assert!(Kit::parse(unfinished)
            .expect_err("no end")
            .message
            .contains("no `end`"));
        assert!(Kit::parse("label x\n")
            .expect_err("no header")
            .message
            .contains("not a kit file"));
    }

    #[test]
    fn an_inconsistent_kit_is_refused() {
        let broken = |change: fn(&mut Kit)| {
            let mut kit = kit();
            change(&mut kit);
            kit.validate().expect_err("must be refused").message
        };
        assert!(broken(|kit| kit.actions[1].id = ActionId(1)).contains("id is used twice"));
        assert!(
            broken(|kit| kit.actions[1].name = "Swing one".into()).contains("name is used twice")
        );
        assert!(broken(|kit| kit.actions[0].next_light = Some(ActionId(9)))
            .contains("itself or missing"));
        assert!(broken(|kit| kit.actions[0].next_heavy = Some(ActionId(1)))
            .contains("itself or missing"));
        assert!(broken(|kit| kit.actions[0].hits.clear()).contains("at least one hit"));
        assert!(broken(|kit| kit.actions[2].iframes = None).contains("i-frames"));
        assert!(broken(|kit| kit.actions[0].hits[0].to = Milli::whole(6)).contains("window"));
        assert!(broken(|kit| kit.actions[0].total = Milli::ZERO).contains("total"));
        assert!(broken(|kit| kit.frame_rate_hz = 0).contains("frame_rate"));
        assert!(broken(|kit| kit.actions.clear()).contains("at least one action"));
    }

    /// 30 frames a second into 100 ticks a second: a frame is three and a third ticks, rounded
    /// to the nearest tick.
    #[test]
    fn a_kit_compiles_to_engine_ticks() {
        let rate = TickRate::new(100).expect("100 Hz");
        let table = kit().to_table(ActionRevision(7), rate).expect("table");
        assert_eq!(table.len(), 3);
        assert_eq!(table.revision, ActionRevision(7));

        let swing = table.get(ActionId(1)).expect("swing");
        assert_eq!(
            *swing,
            ActionSpec {
                id: ActionId(1),
                kind: ActionKind::MeleeLight,
                total_ticks: 100,
                input_from_ticks: 10,
                // The latest of light 40, heavy 50, dodge 30 and guard 60.
                cancel_from_ticks: Some(60),
                iframes: None,
                contact: Some(TickWindow { from: 20, to: 30 }),
                // Both hits: 7 + 4.4, rounded.
                stamina_cost: 11,
                interruptible_until: Some(20),
                followup: Some(ActionId(2)),
            }
        );
        assert_eq!(
            table.inputs(ActionId(1)),
            Some(&InputWindows {
                dodge_input_from: Some(5),
                cancel_light: Some(40),
                cancel_heavy: Some(50),
                cancel_dodge: Some(30),
                cancel_guard: Some(60),
            })
        );

        let roll = table.get(ActionId(3)).expect("roll");
        assert_eq!(roll.kind, ActionKind::Dodge);
        assert_eq!(roll.total_ticks, 80);
        assert_eq!(roll.iframes, Some(TickWindow { from: 0, to: 30 }));
        assert_eq!(roll.contact, None);
        assert_eq!(roll.stamina_cost, 10);
        assert_eq!(roll.interruptible_until, Some(80));
        assert_eq!(roll.cancel_from_ticks, Some(70));
        assert_eq!(
            table.inputs(ActionId(3)).expect("inputs").cancel_guard,
            None
        );
    }

    /// At 60 ticks a second a frame is exactly two ticks; offsets past the end become never and
    /// a window that would vanish keeps one tick.
    #[test]
    fn offsets_past_the_end_are_never_and_windows_keep_a_tick() {
        let rate = TickRate::new(60).expect("60 Hz");
        let mut action = swing(1, "Edge");
        action.cancel.heavy = Some(m("31"));
        action.cancel.light = Some(m("30"));
        action.input_from = None;
        action.hits.truncate(1);
        action.hits[0].from = m("6.2");
        action.hits[0].to = m("6.4");
        let (spec, inputs) = action.compile(30, rate).expect("compile");
        assert_eq!(spec.total_ticks, 60);
        assert_eq!(
            spec.input_from_ticks, 60,
            "a never-listening action hears nothing before its end"
        );
        assert_eq!(
            inputs.cancel_light,
            Some(60),
            "an offset at the end is kept"
        );
        assert_eq!(inputs.cancel_heavy, None, "an offset past the end is never");
        assert_eq!(spec.contact, Some(TickWindow { from: 12, to: 13 }));

        action.hits[0].from = m("30");
        action.hits[0].to = m("31");
        let error = action.compile(30, rate).expect_err("no room");
        assert!(error.message.contains("does not fit"), "{error}");
    }

    #[test]
    fn root_motion_is_interpolated_and_held_at_the_ends() {
        let action = swing(1, "Mover");
        assert_eq!(action.motion_at(m("0")), [m("0"), m("0"), m("0")]);
        assert_eq!(action.motion_at(m("0.5")), [m("0.05"), m("0"), m("0.25")]);
        assert_eq!(action.motion_at(m("1.25")), [m("0.1"), m("0"), m("0.75")]);
        assert_eq!(action.motion_at(m("40")), [m("0.1"), m("0"), m("1.5")]);
        assert_eq!(roll(2).motion_at(m("3")), [Milli::ZERO; 3]);

        let kit = kit();
        let rate = TickRate::new(100).expect("100 Hz");
        assert_eq!(kit.frame_at_tick(rate, 0), Milli::ZERO);
        assert_eq!(kit.frame_at_tick(rate, 10), m("3"));
        assert_eq!(kit.frame_at_tick(rate, 5), m("1.5"));
    }
}
