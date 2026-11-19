//! Build a [`Kit`] from the owner's locally generated Elden Ring action data.
//!
//! The donor project (Funny-Bones/ELDEN-RING-Combat-Rewrite, MIT) has a script that reads the
//! owner's own copy of the game and writes a Rust source file of action tables. That file is
//! game-derived data: it exists only on the owner's machine and is never part of this
//! repository. This module reads it **as text** — the two `match` tables `fn base` and
//! `fn attack`, the weapon list, and a few constants — and picks out one weapon's kit.
//!
//! It is a reader for that one generated shape, not a Rust parser. It understands literals,
//! paths, tuples, slices, struct literals and call-shaped patterns, which is everything the
//! generator emits, and refuses anything else with the line it stopped at.
//!
//! What lands in the kit:
//!
//! - `Backstep` and the four `Roll(<load>, <direction>)` actions, as dodges. Their stamina cost
//!   is not in the action record; it comes from `ROLL_COST` / `BACKSTEP_COST`.
//! - Every ground attack of the chosen weapon and grip, as light or heavy attacks.
//! - The landing halves of the jump attacks (`JumpLightLand`, `JumpHeavyLand`; `PairedJumpLand`
//!   with the off hand). The landing carries the hit; the client plays it when a press made in
//!   the air comes down.
//! - `Parry`, authored here: the donor data has no parry (in ER it is a shield skill). A dodge-
//!   kind action whose `iframes` row is the parry window; every number is an estimate, to tune
//!   in play.
//! - The chain rules of the donor's player code, as data: a light swing continues into the next
//!   one and wraps to the first after the last; running, rolling, backstep and crouch attacks
//!   continue into the second swing; a roll continues into the roll attack and a backstep into
//!   the backstep attack; the first heavy continues into the second charged heavy.
//!
//! What does not, each reported by name in [`ImportReport::skipped`]: the short landings (they
//! follow an air swing that already finished, which the client does not model), off-hand and
//! paired attacks unless asked for, and any record
//! the engine cannot run — an attack without a hit, a dodge without i-frames, an action that
//! costs nothing. Movement stops, landings, jumps, crouch rolls and hit reactions in `fn base`
//! are not kit actions and are only counted.

use std::collections::BTreeMap;

use crate::action::{ActionId, ActionKind};
use crate::kit::{Kit, KitAction, KitCancels, KitError, KitHit, Milli};

/// What to take from the generated data.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ImportOptions<'a> {
    /// A weapon name from the generated weapon list (case-insensitive), or its index.
    pub weapon: &'a str,
    /// Take the two-handed moveset instead of the one-handed one.
    pub two_handed: bool,
    /// The equip-load class whose rolls to take, as the data names it (`Light`, `Medium`,
    /// `Heavy`; case-insensitive).
    pub load: &'a str,
    /// Also take left-hand and paired attacks.
    pub off_hand: bool,
    /// The id of the first action; the rest follow in order. Host ids below 100 are in use.
    pub first_id: u32,
}

impl Default for ImportOptions<'_> {
    fn default() -> Self {
        Self {
            weapon: "1",
            two_handed: false,
            load: "Medium",
            off_hand: false,
            first_id: 100,
        }
    }
}

/// What an import took and what it left.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ImportReport {
    pub weapon_index: usize,
    /// Empty when the data has no weapon list.
    pub weapon_name: String,
    pub dodges: usize,
    pub attacks: usize,
    /// Records for this weapon and load that were left out, with the reason.
    pub skipped: Vec<(String, &'static str)>,
    /// Records in `fn base` that are not dodges at all.
    pub other_base_records: usize,
    /// Windows the data marks as absent by giving them no length.
    pub empty_windows_dropped: usize,
}

/// Read one weapon's kit out of the generated action tables.
///
/// `extracted` is the text of the generated file. `constants` is the text of the hand-written
/// constants file that sits beside it (`data.rs`), which supplies the animation frame rate and
/// the dodge costs; constants are looked up in both.
pub fn import_kit(
    extracted: &str,
    constants: Option<&str>,
    options: &ImportOptions<'_>,
) -> Result<(Kit, ImportReport), KitError> {
    let mut consts = scan_consts(extracted);
    if let Some(constants) = constants {
        consts.extend(scan_consts(constants));
    }
    let constant = |name: &str| {
        consts.get(name).copied().ok_or_else(|| {
            KitError::general(format!(
                "constant `{name}` was not found; pass the constants file that sits beside the generated one"
            ))
        })
    };
    let frame_rate = constant("ANIM_FPS")?;
    let frame_rate_hz = u16::try_from(frame_rate.0 / 1000)
        .ok()
        .filter(|&hz| hz > 0 && frame_rate.0 % 1000 == 0)
        .ok_or_else(|| KitError::general("`ANIM_FPS` must be a whole positive number"))?;
    let roll_cost = constant("ROLL_COST")?;
    let backstep_cost = constant("BACKSTEP_COST")?;
    let never = consts.get("NEVER").copied().unwrap_or(Milli::whole(9999));

    let mut parser = Parser::new(extracted, never)?;
    let weapons = parser.weapon_names()?;
    let weapon_index = match options.weapon.parse::<usize>() {
        Ok(index) => index,
        Err(_) => weapons
            .iter()
            .position(|name| name.eq_ignore_ascii_case(options.weapon))
            .ok_or_else(|| {
                KitError::general(format!(
                    "weapon `{}` is not in the generated list ({})",
                    options.weapon,
                    weapons.join(", ")
                ))
            })?,
    };
    let weapon_name = weapons.get(weapon_index).cloned().unwrap_or_default();
    let mut report = ImportReport {
        weapon_index,
        weapon_name: weapon_name.clone(),
        ..ImportReport::default()
    };

    // `fn base`: the dodges.
    let mut drafts: Vec<Draft> = Vec::new();
    let mut loads_seen: Vec<String> = Vec::new();
    for (pattern, line) in parser.arms("base", |pattern| base_role(pattern).is_some())? {
        let (pattern, def) = match pattern {
            Arm::Taken(pattern, def) => (pattern, def),
            Arm::Skipped => {
                report.other_base_records += 1;
                continue;
            }
        };
        match base_role(&pattern) {
            Some(BaseRole::Backstep) => drafts.push(Draft {
                name: "Backstep".to_string(),
                kind: ActionKind::Dodge,
                cost: Some(backstep_cost),
                def: parser.action_def(&def, line)?,
            }),
            Some(BaseRole::Roll { load, direction }) => {
                if !loads_seen.iter().any(|seen| seen == load) {
                    loads_seen.push(load.to_string());
                }
                if load.eq_ignore_ascii_case(options.load) {
                    drafts.push(Draft {
                        name: format!("Roll{direction}"),
                        kind: ActionKind::Dodge,
                        cost: Some(roll_cost),
                        def: parser.action_def(&def, line)?,
                    });
                }
            }
            None => report.other_base_records += 1,
        }
    }
    if !drafts.iter().any(|draft| draft.name.starts_with("Roll")) {
        return Err(KitError::general(format!(
            "no rolls for load `{}` in the generated data (it has: {})",
            options.load,
            loads_seen.join(", ")
        )));
    }

    // `fn attack`: this weapon and grip.
    let wanted_index = i32::try_from(weapon_index).ok().map(Milli::whole);
    let wanted_grip = if options.two_handed { "true" } else { "false" };
    let arms = parser.arms("attack", |pattern| {
        attack_kind(pattern)
            .is_some_and(|(index, grip, _)| Some(index) == wanted_index && grip == wanted_grip)
    })?;
    for (arm, line) in arms {
        let Arm::Taken(pattern, def) = arm else {
            continue;
        };
        let Some((_, _, kind_name)) = attack_kind(&pattern) else {
            continue;
        };
        if kind_name.contains("Jump") && kind_name.ends_with("Short") {
            report.skipped.push((
                kind_name.to_string(),
                "a short landing, after a finished air swing",
            ));
            continue;
        }
        let paired_weapon = ["Fist", "Claw"]
            .iter()
            .any(|name| weapon_name.eq_ignore_ascii_case(name));
        if (kind_name.starts_with("Left") && (!options.off_hand || options.two_handed))
            || (kind_name.starts_with("Paired")
                && (!options.off_hand || options.two_handed || !paired_weapon))
        {
            report.skipped.push((
                kind_name.to_string(),
                "the selected weapon, grip or off-hand option does not use this row",
            ));
            continue;
        }
        let kind = if kind_name.contains("Heavy") || kind_name == "GuardCounter" {
            ActionKind::MeleeHeavy
        } else {
            ActionKind::MeleeLight
        };
        drafts.push(Draft {
            name: kind_name.to_string(),
            kind,
            cost: None,
            def: parser.action_def(&def, line)?,
        });
    }
    report.empty_windows_dropped = parser.empty_windows;
    if !options.two_handed {
        drafts.push(parry(backstep_cost));
    }

    // Leave out what the engine cannot run, then number what is left.
    drafts.retain(|draft| {
        let reason = match draft.kind {
            ActionKind::Dodge if draft.def.iframes.is_none() => Some("a dodge without i-frames"),
            ActionKind::Dodge if draft.cost.is_some_and(|cost| cost.0 <= 0) => {
                Some("costs no stamina")
            }
            ActionKind::Dodge => None,
            _ if draft.def.hits.is_empty() => Some("an attack without a hit"),
            _ if draft
                .def
                .hits
                .iter()
                .map(|hit| i64::from(hit.stamina.0))
                .sum::<i64>()
                < 500 =>
            {
                Some("costs no stamina")
            }
            _ => None,
        };
        if let Some(reason) = reason {
            report.skipped.push((draft.name.clone(), reason));
        }
        reason.is_none()
    });
    if !drafts.iter().any(|draft| draft.kind != ActionKind::Dodge) {
        return Err(KitError::general(format!(
            "no usable attacks for weapon {weapon_index} ({}) in the generated data",
            if options.two_handed {
                "two-handed"
            } else {
                "one-handed"
            }
        )));
    }
    let ids: BTreeMap<String, ActionId> = drafts
        .iter()
        .zip(options.first_id..)
        .map(|(draft, id)| (draft.name.clone(), ActionId(id)))
        .collect();
    let id_of = |name: &str| ids.get(name).copied();

    let actions = drafts
        .into_iter()
        .map(|draft| {
            let id = ids[&draft.name];
            let (next_light, next_heavy) = continuations(&draft.name, &id_of);
            let def = draft.def;
            KitAction {
                id,
                name: draft.name,
                source: def.source,
                kind: draft.kind,
                total: def.total,
                input_from: def.input_from,
                dodge_input_from: def.input_dodge_from,
                cancel: def.cancel,
                iframes: def.iframes,
                airborne: def.jump_frames,
                stamina: draft.cost.unwrap_or(def.stamina),
                hits: def.hits,
                charge: def.charge,
                no_turn: def.no_turn,
                turn: def.turn,
                motion: def.motion,
                next_light: next_light.filter(|&next| next != id),
                next_heavy: next_heavy.filter(|&next| next != id),
            }
        })
        .collect::<Vec<_>>();
    report.dodges = actions
        .iter()
        .filter(|action| action.kind == ActionKind::Dodge)
        .count();
    report.attacks = actions.len() - report.dodges;

    let weapon_label = if weapon_name.is_empty() {
        format!("weapon {weapon_index}")
    } else {
        weapon_name
    };
    let kit = Kit {
        label: format!(
            "{weapon_label}, {}, {} load",
            if options.two_handed {
                "two-handed"
            } else {
                "one-handed"
            },
            options.load.to_ascii_lowercase()
        ),
        frame_rate_hz,
        stamina_max: consts.get("MAX_STAMINA").copied(),
        actions,
    };
    kit.validate()?;
    Ok((kit, report))
}

/// The donor's chain rules, as `(what a light press continues into, what a heavy press does)`.
fn continuations(
    name: &str,
    id_of: &dyn Fn(&str) -> Option<ActionId>,
) -> (Option<ActionId>, Option<ActionId>) {
    let chain = |family: &str| {
        let step = name.strip_prefix(family)?.parse::<u32>().ok()?;
        id_of(&format!("{family}{}", step + 1)).or_else(|| id_of(&format!("{family}1")))
    };
    let next_light = if name.starts_with("Roll") && name != "RollAttack" {
        id_of("RollAttack")
    } else {
        match name {
            "Backstep" => id_of("BackstepAttack"),
            "RunLight" | "RollAttack" | "BackstepAttack" | "CrouchAttack" => id_of("Light2"),
            _ => chain("Light")
                .or_else(|| chain("LeftLight"))
                .or_else(|| chain("PairedLight")),
        }
    };
    let next_heavy = match name {
        "Heavy1" | "Heavy1Charge" => id_of("Heavy2Charge"),
        _ => None,
    };
    (next_light, next_heavy)
}

/// The authored parry (ESTIMATE, in 30 fps frames): a 40-frame action whose window is frames 6
/// to 14, that the dodge can cancel from 24 and anything from 30; it costs a backstep.
fn parry(cost: Milli) -> Draft {
    let frame = Milli::whole;
    Draft {
        name: "Parry".to_string(),
        kind: ActionKind::Dodge,
        cost: Some(cost),
        def: ActionDef {
            source: "authored_parry".to_string(),
            total: frame(40),
            input_from: Some(frame(24)),
            input_dodge_from: Some(frame(24)),
            cancel: KitCancels {
                light: Some(frame(30)),
                heavy: Some(frame(30)),
                dodge: Some(frame(24)),
                jump: Some(frame(30)),
                guard: Some(frame(24)),
                movement: Some(frame(30)),
                left: Some(frame(30)),
            },
            iframes: Some((frame(6), frame(14))),
            jump_frames: false,
            stamina: Milli::ZERO,
            hits: Vec::new(),
            charge: None,
            no_turn: vec![(Milli::ZERO, frame(30))],
            turn: Vec::new(),
            motion: Vec::new(),
        },
    }
}

struct Draft {
    name: String,
    kind: ActionKind,
    /// A dodge's cost, which the action record does not carry.
    cost: Option<Milli>,
    def: ActionDef,
}

/// One generated `ActionDef`, read.
struct ActionDef {
    source: String,
    total: Milli,
    input_from: Option<Milli>,
    input_dodge_from: Option<Milli>,
    cancel: KitCancels,
    iframes: Option<(Milli, Milli)>,
    jump_frames: bool,
    stamina: Milli,
    hits: Vec<KitHit>,
    charge: Option<(Milli, Milli)>,
    no_turn: Vec<(Milli, Milli)>,
    turn: Vec<(Milli, Milli, Milli)>,
    motion: Vec<[Milli; 3]>,
}

enum BaseRole<'a> {
    Backstep,
    Roll { load: &'a str, direction: &'a str },
}

/// Which kit action a `fn base` pattern is: `Backstep`, or `Roll(Load::X, Dir::Y)`.
fn base_role<'v>(pattern: &'v Value<'_>) -> Option<BaseRole<'v>> {
    match pattern {
        Value::Path(path) if path.as_slice() == ["Backstep"] => Some(BaseRole::Backstep),
        Value::Call { name, args } if name.as_slice() == ["Roll"] => match args.as_slice() {
            [Value::Path(load), Value::Path(direction)] => Some(BaseRole::Roll {
                load: load.last().copied()?,
                direction: direction.last().copied()?,
            }),
            _ => None,
        },
        _ => None,
    }
}

/// The `(weapon, two_hand, kind)` of a `fn attack` pattern.
fn attack_kind<'v>(pattern: &'v Value<'_>) -> Option<(Milli, &'v str, &'v str)> {
    match pattern {
        Value::Tuple(parts) => match parts.as_slice() {
            [Value::Num(index), Value::Path(grip), Value::Path(kind)] => {
                Some((*index, grip.last().copied()?, kind.last().copied()?))
            }
            _ => None,
        },
        _ => None,
    }
}

/// `pub const NAME: <type> = <number>;` lines. Anything that is not a plain number is left out.
fn scan_consts(text: &str) -> BTreeMap<&str, Milli> {
    text.lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("pub const ")?;
            let (name, rest) = rest.split_once(':')?;
            let (_, value) = rest.split_once('=')?;
            let value = value.split_once(';')?.0.trim();
            Some((name.trim(), Milli::parse(value)?))
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Tok<'a> {
    Ident(&'a str),
    Num(&'a str),
    Str(&'a str),
    /// `=>`
    Arrow,
    /// `::`
    PathSep,
    Punct(char),
}

#[derive(Clone, Copy, Debug)]
struct Token<'a> {
    tok: Tok<'a>,
    line: usize,
}

/// Split the text into tokens. Comments are dropped; every other character becomes part of a
/// token, so this cannot fail on text it does not understand — the reader above it does.
fn tokenize(text: &str) -> Result<Vec<Token<'_>>, KitError> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut line = 1;
    let mut at = 0;
    while at < bytes.len() {
        let byte = bytes[at];
        let start = at;
        let tok = match byte {
            b'\n' => {
                line += 1;
                at += 1;
                continue;
            }
            _ if byte.is_ascii_whitespace() => {
                at += 1;
                continue;
            }
            b'/' if bytes.get(at + 1) == Some(&b'/') => {
                while at < bytes.len() && bytes[at] != b'\n' {
                    at += 1;
                }
                continue;
            }
            b'"' => {
                at += 1;
                while at < bytes.len() && bytes[at] != b'"' {
                    if bytes[at] == b'\n' {
                        line += 1;
                    }
                    at += if bytes[at] == b'\\' { 2 } else { 1 };
                }
                if at >= bytes.len() {
                    return Err(KitError::at(line, "a string is not closed"));
                }
                at += 1;
                Tok::Str(&text[start + 1..at - 1])
            }
            _ if byte.is_ascii_digit() => {
                while at < bytes.len() && bytes[at].is_ascii_digit() {
                    at += 1;
                }
                if bytes.get(at) == Some(&b'.') && bytes.get(at + 1).is_some_and(u8::is_ascii_digit)
                {
                    at += 1;
                    while at < bytes.len() && bytes[at].is_ascii_digit() {
                        at += 1;
                    }
                }
                Tok::Num(&text[start..at])
            }
            _ if byte.is_ascii_alphabetic() || byte == b'_' => {
                while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
                    at += 1;
                }
                Tok::Ident(&text[start..at])
            }
            b'=' if bytes.get(at + 1) == Some(&b'>') => {
                at += 2;
                Tok::Arrow
            }
            b':' if bytes.get(at + 1) == Some(&b':') => {
                at += 2;
                Tok::PathSep
            }
            _ if byte.is_ascii() => {
                at += 1;
                Tok::Punct(char::from(byte))
            }
            _ => {
                // Outside strings and comments the generator writes ASCII only; step over the
                // whole character so slicing stays on a boundary.
                let width = text[at..].chars().next().map_or(1, char::len_utf8);
                at += width;
                Tok::Punct('\u{fffd}')
            }
        };
        tokens.push(Token { tok, line });
    }
    Ok(tokens)
}

/// A value as the generator writes them.
#[derive(Clone, PartialEq, Debug)]
enum Value<'a> {
    Num(Milli),
    Str(&'a str),
    /// `false`, `None`, `NEVER`, `Dir::Front`
    Path(Vec<&'a str>),
    /// `[a, b]`
    List(Vec<Value<'a>>),
    /// `(a, b)`
    Tuple(Vec<Value<'a>>),
    /// `Name { field: value }`, with the line each field is on.
    Struct {
        name: &'a str,
        fields: Vec<(&'a str, usize, Value<'a>)>,
    },
    /// `Some(x)`, `Roll(Load::Medium, Dir::Front)`
    Call {
        name: Vec<&'a str>,
        args: Vec<Value<'a>>,
    },
}

enum Arm<'a> {
    /// The pattern and its value.
    Taken(Value<'a>, Value<'a>),
    Skipped,
}

struct Parser<'a> {
    tokens: Vec<Token<'a>>,
    at: usize,
    /// Times at or past this are the data's way of writing "never".
    never: Milli,
    empty_windows: usize,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str, never: Milli) -> Result<Self, KitError> {
        Ok(Self {
            tokens: tokenize(text)?,
            at: 0,
            never,
            empty_windows: 0,
        })
    }

    fn peek(&self) -> Option<Tok<'a>> {
        self.tokens.get(self.at).map(|token| token.tok)
    }

    fn line(&self) -> usize {
        self.tokens
            .get(self.at)
            .or(self.tokens.last())
            .map_or(0, |token| token.line)
    }

    fn fail<T>(&self, message: impl Into<String>) -> Result<T, KitError> {
        Err(KitError::at(self.line(), message))
    }

    fn eat(&mut self, tok: Tok<'_>) -> bool {
        let found = self.peek() == Some(tok);
        if found {
            self.at += 1;
        }
        found
    }

    fn expect(&mut self, tok: Tok<'_>, what: &str) -> Result<(), KitError> {
        if self.eat(tok) {
            Ok(())
        } else {
            self.fail(format!("expected {what}"))
        }
    }

    /// Move to just after the first place `sequence` appears, searching from the start.
    fn seek(&mut self, sequence: &[Tok<'_>]) -> bool {
        let found = self.tokens.windows(sequence.len()).position(|window| {
            window
                .iter()
                .map(|token| token.tok)
                .eq(sequence.iter().copied())
        });
        if let Some(index) = found {
            self.at = index + sequence.len();
        }
        found.is_some()
    }

    /// Move to just after the next `tok`.
    fn skip_past(&mut self, tok: Tok<'_>, what: &str) -> Result<(), KitError> {
        while let Some(found) = self.peek() {
            self.at += 1;
            if found == tok {
                return Ok(());
            }
        }
        self.fail(format!("expected {what} before the end of the file"))
    }

    fn list(&mut self, close: char) -> Result<Vec<Value<'a>>, KitError> {
        let mut items = Vec::new();
        loop {
            if self.eat(Tok::Punct(close)) {
                return Ok(items);
            }
            items.push(self.value()?);
            if !self.eat(Tok::Punct(',')) {
                self.expect(Tok::Punct(close), &format!("`,` or `{close}`"))?;
                return Ok(items);
            }
        }
    }

    fn value(&mut self) -> Result<Value<'a>, KitError> {
        while self.eat(Tok::Punct('&')) {}
        let negative = self.eat(Tok::Punct('-'));
        let Some(tok) = self.peek() else {
            return self.fail("the file ends in the middle of a value");
        };
        if negative && !matches!(tok, Tok::Num(_)) {
            return self.fail("expected a number after `-`");
        }
        self.at += 1;
        match tok {
            Tok::Num(text) => match Milli::parse(text) {
                Some(number) => Ok(Value::Num(if negative { Milli(-number.0) } else { number })),
                None => {
                    self.at -= 1;
                    self.fail(format!("`{text}` is not a number this reader handles"))
                }
            },
            Tok::Str(text) => Ok(Value::Str(text)),
            Tok::Punct('[') => Ok(Value::List(self.list(']')?)),
            Tok::Punct('(') => Ok(Value::Tuple(self.list(')')?)),
            Tok::Ident(first) => {
                let mut path = vec![first];
                while self.eat(Tok::PathSep) {
                    match self.peek() {
                        Some(Tok::Ident(segment)) => {
                            self.at += 1;
                            path.push(segment);
                        }
                        _ => return self.fail("expected a name after `::`"),
                    }
                }
                if self.eat(Tok::Punct('(')) {
                    return Ok(Value::Call {
                        name: path,
                        args: self.list(')')?,
                    });
                }
                if !self.eat(Tok::Punct('{')) {
                    return Ok(Value::Path(path));
                }
                let mut fields = Vec::new();
                loop {
                    if self.eat(Tok::Punct('}')) {
                        break;
                    }
                    let line = self.line();
                    let Some(Tok::Ident(field)) = self.peek() else {
                        return self.fail("expected a field name");
                    };
                    self.at += 1;
                    self.expect(Tok::Punct(':'), "`:` after a field name")?;
                    fields.push((field, line, self.value()?));
                    if !self.eat(Tok::Punct(',')) {
                        self.expect(Tok::Punct('}'), "`,` or `}`")?;
                        break;
                    }
                }
                Ok(Value::Struct {
                    name: path.last().copied().unwrap_or(first),
                    fields,
                })
            }
            _ => {
                self.at -= 1;
                self.fail("expected a value")
            }
        }
    }

    /// Step over one match arm's value: everything up to the comma that ends it.
    fn skip_arm_value(&mut self) -> Result<(), KitError> {
        let mut depth = 0_usize;
        while let Some(tok) = self.peek() {
            match tok {
                Tok::Punct('(' | '[' | '{') => depth += 1,
                Tok::Punct(')' | ']' | '}') => {
                    if depth == 0 {
                        return Ok(());
                    }
                    depth -= 1;
                }
                Tok::Punct(',') if depth == 0 => {
                    self.at += 1;
                    return Ok(());
                }
                _ => {}
            }
            self.at += 1;
        }
        self.fail("the file ends in the middle of a match arm")
    }

    /// The arms of the `match` in `fn <function>`, in file order with the line each starts on.
    /// Arms `take` refuses are stepped over without being read.
    fn arms(
        &mut self,
        function: &str,
        take: impl Fn(&Value<'a>) -> bool,
    ) -> Result<Vec<(Arm<'a>, usize)>, KitError> {
        if !self.seek(&[Tok::Ident("fn"), Tok::Ident(function), Tok::Punct('(')]) {
            return Err(KitError::general(format!(
                "`fn {function}` was not found: this is not the generated action file"
            )));
        }
        self.skip_past(Tok::Ident("match"), "the `match` of the table")?;
        self.skip_past(Tok::Punct('{'), "the `{` of the table")?;
        let mut arms = Vec::new();
        while !self.eat(Tok::Punct('}')) {
            let line = self.line();
            let pattern = self.value()?;
            self.expect(Tok::Arrow, "`=>` after a pattern")?;
            if matches!(&pattern, Value::Path(path) if path.as_slice() == ["_"]) {
                // The catch-all arm is not a record.
                self.skip_arm_value()?;
            } else if take(&pattern) {
                let value = self.value()?;
                if !self.eat(Tok::Punct(',')) && self.peek() != Some(Tok::Punct('}')) {
                    return self.fail("expected `,` after a match arm");
                }
                arms.push((Arm::Taken(pattern, value), line));
            } else {
                self.skip_arm_value()?;
                arms.push((Arm::Skipped, line));
            }
        }
        Ok(arms)
    }

    /// The names in `pub const WEAPONS`, in order; empty when the file has no such list.
    fn weapon_names(&mut self) -> Result<Vec<String>, KitError> {
        if !self.seek(&[Tok::Ident("const"), Tok::Ident("WEAPONS")]) {
            return Ok(Vec::new());
        }
        self.skip_past(Tok::Punct('='), "`=` after `WEAPONS`")?;
        let line = self.line();
        let Value::List(entries) = self.value()? else {
            return Err(KitError::at(line, "`WEAPONS` is not a list"));
        };
        entries
            .iter()
            .map(|entry| match entry {
                Value::Struct { fields, .. } => fields
                    .iter()
                    .find_map(|(field, _, value)| match value {
                        Value::Str(name) if *field == "name" => Some((*name).to_string()),
                        _ => None,
                    })
                    .ok_or_else(|| KitError::at(line, "a weapon has no name")),
                _ => Err(KitError::at(line, "a weapon entry is not a record")),
            })
            .collect()
    }

    fn number(&self, value: &Value<'_>, line: usize, what: &str) -> Result<Milli, KitError> {
        match value {
            Value::Num(number) => Ok(*number),
            _ => Err(KitError::at(line, format!("`{what}` must be a number"))),
        }
    }

    /// A time that may be `NEVER`, by name or by value.
    fn time(&self, value: &Value<'_>, line: usize, what: &str) -> Result<Option<Milli>, KitError> {
        match value {
            Value::Path(path) if path.as_slice() == ["NEVER"] => Ok(None),
            Value::Num(number) => Ok((*number < self.never).then_some(*number)),
            _ => Err(KitError::at(
                line,
                format!("`{what}` must be a number or NEVER"),
            )),
        }
    }

    fn numbers<const N: usize>(
        &self,
        value: &Value<'_>,
        line: usize,
        what: &str,
    ) -> Result<[Milli; N], KitError> {
        let (Value::Tuple(parts) | Value::List(parts)) = value else {
            return Err(KitError::at(
                line,
                format!("`{what}` must be a group of {N} numbers"),
            ));
        };
        if parts.len() != N {
            return Err(KitError::at(
                line,
                format!("`{what}` must be a group of {N} numbers"),
            ));
        }
        let mut numbers = [Milli::ZERO; N];
        for (slot, part) in numbers.iter_mut().zip(parts) {
            *slot = self.number(part, line, what)?;
        }
        Ok(numbers)
    }

    fn items<'v>(
        &self,
        value: &'v Value<'a>,
        line: usize,
        what: &str,
    ) -> Result<&'v [Value<'a>], KitError> {
        match value {
            Value::List(items) => Ok(items),
            _ => Err(KitError::at(line, format!("`{what}` must be a list"))),
        }
    }

    /// A `(from, to)` window; one with no length is the data's "none".
    fn window(
        &mut self,
        value: &Value<'_>,
        line: usize,
        what: &str,
    ) -> Result<Option<(Milli, Milli)>, KitError> {
        let [from, to] = self.numbers(value, line, what)?;
        if from >= to {
            self.empty_windows += 1;
            return Ok(None);
        }
        Ok(Some((from, to)))
    }

    fn hit(&mut self, value: &Value<'a>, line: usize) -> Result<Option<KitHit>, KitError> {
        let Value::Struct {
            name: "Hit",
            fields,
        } = value
        else {
            return Err(KitError::at(line, "`hits` must hold `Hit` records"));
        };
        let mut seen: BTreeMap<&str, &Value<'a>> = BTreeMap::new();
        for (field, field_line, value) in fields {
            const KNOWN: [&str; 8] = [
                "from",
                "to",
                "mv",
                "guard_damage",
                "stamina",
                "stop",
                "radius",
                "blade",
            ];
            if !KNOWN.contains(field) {
                return Err(KitError::at(
                    *field_line,
                    format!("unknown `Hit` field `{field}`"),
                ));
            }
            seen.insert(*field, value);
        }
        let field = |name: &str| {
            seen.get(name)
                .copied()
                .ok_or_else(|| KitError::at(line, format!("a `Hit` has no `{name}`")))
        };
        let from = self.number(field("from")?, line, "from")?;
        let to = self.number(field("to")?, line, "to")?;
        let hit = KitHit {
            from,
            to,
            motion_value: self.number(field("mv")?, line, "mv")?,
            guard_damage: self.number(field("guard_damage")?, line, "guard_damage")?,
            stamina: self.number(field("stamina")?, line, "stamina")?,
            hit_stop: self.number(field("stop")?, line, "stop")?,
            radius: self.number(field("radius")?, line, "radius")?,
            blade: self
                .items(field("blade")?, line, "blade")?
                .iter()
                .map(|sample| self.numbers(sample, line, "blade"))
                .collect::<Result<_, _>>()?,
        };
        if from >= to {
            self.empty_windows += 1;
            return Ok(None);
        }
        Ok(Some(hit))
    }

    /// Read one `ActionDef` record. Every field the generator writes must be present, and a
    /// field this reader does not know is an error rather than something to lose silently.
    fn action_def(&mut self, value: &Value<'a>, line: usize) -> Result<ActionDef, KitError> {
        let Value::Struct {
            name: "ActionDef",
            fields,
        } = value
        else {
            return Err(KitError::at(line, "expected an `ActionDef` record"));
        };
        let mut seen: BTreeMap<&str, &Value<'a>> = BTreeMap::new();
        for (field, field_line, value) in fields {
            const KNOWN: [&str; 20] = [
                "name",
                "source",
                "total",
                "input_from",
                "input_dodge_from",
                "cancel_light",
                "cancel_heavy",
                "cancel_dodge",
                "cancel_jump",
                "cancel_guard",
                "cancel_move",
                "cancel_left",
                "iframes",
                "jump_frames",
                "stamina",
                "hits",
                "charge",
                "no_turn",
                "turn",
                "motion",
            ];
            if !KNOWN.contains(field) {
                return Err(KitError::at(
                    *field_line,
                    format!("unknown `ActionDef` field `{field}`"),
                ));
            }
            seen.insert(*field, value);
        }
        let field = |name: &str| {
            seen.get(name)
                .copied()
                .ok_or_else(|| KitError::at(line, format!("an `ActionDef` has no `{name}`")))
        };

        let source = match field("source")? {
            Value::Str(source) => (*source).to_string(),
            _ => return Err(KitError::at(line, "`source` must be a string")),
        };
        let jump_frames = match field("jump_frames")? {
            Value::Path(path) if path.as_slice() == ["true"] => true,
            Value::Path(path) if path.as_slice() == ["false"] => false,
            _ => return Err(KitError::at(line, "`jump_frames` must be true or false")),
        };
        let charge = match field("charge")? {
            Value::Path(path) if path.as_slice() == ["None"] => None,
            Value::Call { name, args } if name.as_slice() == ["Some"] && args.len() == 1 => {
                self.window(&args[0], line, "charge")?
            }
            _ => {
                return Err(KitError::at(
                    line,
                    "`charge` must be None or Some((from, to))",
                ))
            }
        };
        let mut hits = Vec::new();
        for hit in self.items(field("hits")?, line, "hits")? {
            hits.extend(self.hit(hit, line)?);
        }
        let mut no_turn = Vec::new();
        for window in self.items(field("no_turn")?, line, "no_turn")? {
            no_turn.extend(self.window(window, line, "no_turn")?);
        }
        let mut turn = Vec::new();
        for entry in self.items(field("turn")?, line, "turn")? {
            let [from, to, rate] = self.numbers(entry, line, "turn")?;
            if from >= to {
                self.empty_windows += 1;
            } else {
                turn.push((from, to, rate));
            }
        }
        let motion = self
            .items(field("motion")?, line, "motion")?
            .iter()
            .map(|sample| self.numbers(sample, line, "motion"))
            .collect::<Result<_, _>>()?;

        Ok(ActionDef {
            source,
            total: self.number(field("total")?, line, "total")?,
            input_from: self.time(field("input_from")?, line, "input_from")?,
            input_dodge_from: self.time(field("input_dodge_from")?, line, "input_dodge_from")?,
            cancel: KitCancels {
                light: self.time(field("cancel_light")?, line, "cancel_light")?,
                heavy: self.time(field("cancel_heavy")?, line, "cancel_heavy")?,
                dodge: self.time(field("cancel_dodge")?, line, "cancel_dodge")?,
                jump: self.time(field("cancel_jump")?, line, "cancel_jump")?,
                guard: self.time(field("cancel_guard")?, line, "cancel_guard")?,
                movement: self.time(field("cancel_move")?, line, "cancel_move")?,
                left: self.time(field("cancel_left")?, line, "cancel_left")?,
            },
            iframes: self.window(field("iframes")?, line, "iframes")?,
            jump_frames,
            stamina: self.number(field("stamina")?, line, "stamina")?,
            hits,
            charge,
            no_turn,
            turn,
            motion,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::ActionRevision;
    use crate::time::TickRate;

    /// The generated file's shape with authored values: nothing here is taken from any game.
    fn sample() -> String {
        let def = |name: &str, extra: &str| {
            let clip = name.replace(|c: char| !c.is_ascii_alphanumeric(), "");
            format!(
                "ActionDef {{ name: \"{name}\", source: \"t000_{clip}\", total: 30.0, input_from: 3.0, \
                 input_dodge_from: 1.5, cancel_light: 12.0, cancel_heavy: 15.0, cancel_dodge: 9.0, \
                 cancel_jump: NEVER, cancel_guard: 9999.0, cancel_move: 21.0, cancel_left: 18.0, {extra}, \
                 no_turn: &[(4.0, 33.0), (0.0, 0.0)], turn: &[(0.0, 4.0, 540.0)], \
                 motion: &[[0.000, 0.000, -0.000], [0.100, 0.000, 0.500]] }}"
            )
        };
        let dodge = |name: &str| {
            def(
                name,
                "iframes: (0.0, 9.0), jump_frames: true, stamina: 0.0, hits: &[], charge: None",
            )
        };
        let attack = |name: &str, cost: &str, charge: &str| {
            def(
                name,
                &format!(
                    "iframes: (0.0, 0.0), jump_frames: false, stamina: {cost}, hits: &[Hit {{ from: 6.0, to: 9.0, \
                     mv: 1.00, guard_damage: 0.50, stamina: {cost}, stop: 0.050, radius: 0.125, \
                     blade: &[[-0.500, 1.000, 0.250, -1.500, 1.125, 0.000]] }}], charge: {charge}"
                ),
            )
        };
        let mut text = String::from(
            "//! GENERATED. It's \"quoted\" in a comment.\n\
             use super::data::{ActionDef, NEVER};\n\n\
             pub const MAX_STAMINA: f32 = 80.0;\n\
             pub const WEAPONS: &[WeaponInfo] = &[\n    \
             WeaponInfo { name: \"Test Knife\", category: 1, attack: 10.0, weight: 1.0, stance: [0, 1] },\n    \
             WeaponInfo { name: \"Test Blade\", category: 2, attack: 20.0, weight: 2.0, stance: [0, 1] },\n];\n\
             pub const DEFAULT_WEAPON: usize = 1;\n\n\
             #[rustfmt::skip]\n\
             pub fn base(id: ActionId) -> Option<ActionDef> {\n    use ActionId::*;\n    Some(match id {\n",
        );
        text.push_str(&format!("        Backstep => {},\n", dodge("Backstep")));
        text.push_str(&format!("        SprintStop => {},\n", dodge("SprintStop")));
        for load in ["Light", "Medium"] {
            for direction in ["Front", "Back", "Left", "Right"] {
                text.push_str(&format!(
                    "        Roll(Load::{load}, Dir::{direction}) => {},\n",
                    dodge(&format!("Roll({load}, {direction})"))
                ));
            }
        }
        text.push_str("        _ => return None,\n    })\n}\n\n#[rustfmt::skip]\n");
        text.push_str(
            "pub fn attack(weapon: usize, two_hand: bool, kind: AttackKind) -> Option<ActionDef> {\n    \
             use AttackKind::*;\n    Some(match (weapon, two_hand, kind) {\n",
        );
        let arms = [
            ("0, false, Light1", attack("Light1", "5.0", "None")),
            ("1, false, Light1", attack("Light1", "7.0", "None")),
            ("1, false, Light2", attack("Light2", "7.0", "None")),
            ("1, false, RollAttack", attack("RollAttack", "8.0", "None")),
            (
                "1, false, BackstepAttack",
                attack("BackstepAttack", "8.0", "None"),
            ),
            (
                "1, false, Heavy1Charge",
                attack("Heavy1Charge", "14.0", "Some((3.0, 5.0))"),
            ),
            ("1, false, Heavy1", attack("Heavy1", "12.0", "None")),
            (
                "1, false, Heavy2Charge",
                attack("Heavy2Charge", "14.0", "Some((0.0, 0.0))"),
            ),
            (
                "1, false, JumpLightLand",
                attack("JumpLightLand", "7.0", "None"),
            ),
            (
                "1, false, JumpLightLandShort",
                attack("JumpLightLandShort", "7.0", "None"),
            ),
            ("1, false, LeftLight1", attack("LeftLight1", "7.0", "None")),
            ("1, false, Feint", attack("Feint", "0.0", "None")),
            ("1, true, Light1", attack("Light1", "9.0", "None")),
        ];
        for (pattern, def) in arms {
            text.push_str(&format!("        ({pattern}) => {def},\n"));
        }
        text.push_str("        _ => return None,\n    })\n}\n\npub fn air_attack() {}\n");
        text
    }

    const CONSTANTS: &str = "pub const ANIM_FPS: f32 = 30.0;\npub const DT: f32 = 1.0 / 60.0;\n\
                             pub const DF: f32 = ANIM_FPS * DT;\n/// ESTIMATE.\npub const ROLL_COST: f32 = 10.0;\n\
                             pub const BACKSTEP_COST: f32 = 6.0;\npub const NEVER: f32 = 9999.0;\n";

    fn options() -> ImportOptions<'static> {
        ImportOptions {
            weapon: "test blade",
            ..ImportOptions::default()
        }
    }

    #[test]
    fn one_weapon_is_read_out_of_the_generated_shape() {
        let (kit, report) = import_kit(&sample(), Some(CONSTANTS), &options()).expect("import");
        assert_eq!(kit.label, "Test Blade, one-handed, medium load");
        assert_eq!(kit.frame_rate_hz, 30);
        assert_eq!(kit.stamina_max, Some(Milli::whole(80)));
        let names: Vec<&str> = kit
            .actions
            .iter()
            .map(|action| action.name.as_str())
            .collect();
        assert_eq!(
            names,
            [
                "Backstep",
                "RollFront",
                "RollBack",
                "RollLeft",
                "RollRight",
                "Light1",
                "Light2",
                "RollAttack",
                "BackstepAttack",
                "Heavy1Charge",
                "Heavy1",
                "Heavy2Charge",
                "JumpLightLand",
                "Parry"
            ]
        );
        assert_eq!(kit.actions[0].id, ActionId(100));
        assert_eq!(kit.actions[13].id, ActionId(113));
        assert_eq!(
            report,
            ImportReport {
                weapon_index: 1,
                weapon_name: "Test Blade".to_string(),
                dodges: 6,
                attacks: 8,
                skipped: vec![
                    (
                        "JumpLightLandShort".to_string(),
                        "a short landing, after a finished air swing"
                    ),
                    (
                        "LeftLight1".to_string(),
                        "the selected weapon, grip or off-hand option does not use this row"
                    ),
                    ("Feint".to_string(), "costs no stamina"),
                ],
                // SprintStop. Rolls of another load are dodges, just not the ones asked for.
                other_base_records: 1,
                // Per attack: the i-frames and one turn lock; per dodge: one turn lock; and the
                // second heavy's charge.
                empty_windows_dropped: 9 * 2 + 5 + 1,
            }
        );

        let light = kit.action(ActionId(105)).expect("Light1");
        assert_eq!(light.kind, ActionKind::MeleeLight);
        assert_eq!(light.source, "t000_Light1");
        assert_eq!(light.total, Milli::whole(30));
        assert_eq!(light.input_from, Some(Milli::whole(3)));
        assert_eq!(light.dodge_input_from, Some(Milli(1500)));
        assert_eq!(light.cancel.jump, None, "NEVER by name");
        assert_eq!(light.cancel.guard, None, "NEVER by value");
        assert_eq!(light.cancel.movement, Some(Milli::whole(21)));
        assert_eq!(light.iframes, None);
        assert_eq!(light.no_turn, [(Milli::whole(4), Milli::whole(33))]);
        assert_eq!(
            light.turn,
            [(Milli::ZERO, Milli::whole(4), Milli::whole(540))]
        );
        assert_eq!(
            light.motion,
            [[Milli::ZERO; 3], [Milli(100), Milli::ZERO, Milli(500)]]
        );
        assert_eq!(light.hits.len(), 1);
        assert_eq!(
            light.hits[0].stamina,
            Milli::whole(7),
            "the one-handed record, not the two-handed one"
        );
        assert_eq!(light.hits[0].hit_stop, Milli(50));
        assert_eq!(light.hits[0].blade[0][3], Milli(-1500));

        let roll = kit.action(ActionId(101)).expect("RollFront");
        assert_eq!(roll.kind, ActionKind::Dodge);
        assert_eq!(
            roll.stamina,
            Milli::whole(10),
            "the roll cost comes from the constants"
        );
        assert_eq!(roll.iframes, Some((Milli::ZERO, Milli::whole(9))));
        assert!(roll.airborne);
        assert_eq!(kit.actions[0].stamina, Milli::whole(6));

        let heavy = kit.action(ActionId(109)).expect("Heavy1Charge");
        assert_eq!(heavy.kind, ActionKind::MeleeHeavy);
        assert_eq!(heavy.charge, Some((Milli::whole(3), Milli::whole(5))));

        // What was read is a kit the engine accepts and that survives its own file format.
        assert_eq!(Kit::parse(&kit.to_text()).expect("round trip"), kit);
        let table = kit
            .to_table(ActionRevision(1), TickRate::new(100).expect("100 Hz"))
            .expect("table");
        assert_eq!(table.len(), 14);
    }

    #[test]
    fn chains_follow_the_donor_rules() {
        let (kit, _) = import_kit(&sample(), Some(CONSTANTS), &options()).expect("import");
        let id = |name: &str| {
            kit.actions
                .iter()
                .find(|action| action.name == name)
                .map(|action| action.id)
        };
        let next = |name: &str| {
            let action = kit
                .actions
                .iter()
                .find(|action| action.name == name)
                .expect(name);
            (action.next_light, action.next_heavy)
        };
        assert_eq!(next("Light1"), (id("Light2"), None));
        assert_eq!(
            next("Light2"),
            (id("Light1"), None),
            "past its end the chain starts again"
        );
        assert_eq!(next("RollAttack"), (id("Light2"), None));
        assert_eq!(next("BackstepAttack"), (id("Light2"), None));
        assert_eq!(next("RollLeft"), (id("RollAttack"), None));
        assert_eq!(next("Backstep"), (id("BackstepAttack"), None));
        assert_eq!(next("Heavy1Charge"), (None, id("Heavy2Charge")));
        assert_eq!(next("Heavy1"), (None, id("Heavy2Charge")));
        assert_eq!(next("Heavy2Charge"), (None, None));

        // Other selections: by index, two-handed, the off hand, another load.
        let two_handed = ImportOptions {
            weapon: "1",
            two_handed: true,
            load: "light",
            first_id: 7,
            ..ImportOptions::default()
        };
        let (kit, report) = import_kit(&sample(), Some(CONSTANTS), &two_handed).expect("import");
        assert_eq!(kit.label, "Test Blade, two-handed, light load");
        assert_eq!((report.dodges, report.attacks), (5, 1));
        assert!(kit.action_named("Parry").is_none());
        assert_eq!(kit.actions[5].hits[0].stamina, Milli::whole(9));
        assert_eq!(
            kit.actions[5].next_light, None,
            "a chain of one does not continue into itself"
        );
        assert_eq!(kit.actions[0].id, ActionId(7));

        let off_hand = ImportOptions {
            off_hand: true,
            ..options()
        };
        let (kit, _) = import_kit(&sample(), Some(CONSTANTS), &off_hand).expect("import");
        assert!(kit.actions.iter().any(|action| action.name == "LeftLight1"));
    }

    #[test]
    fn unusable_input_is_refused_with_a_reason() {
        let sample = sample();
        let fails = |text: &str, constants: Option<&str>, options: ImportOptions<'_>| {
            import_kit(text, constants, &options).expect_err("must be refused")
        };
        assert!(fails(&sample, None, options())
            .message
            .contains("`ANIM_FPS` was not found"));
        let unknown = ImportOptions {
            weapon: "Spoon",
            ..options()
        };
        assert!(fails(&sample, Some(CONSTANTS), unknown)
            .message
            .contains("not in the generated list (Test Knife, Test Blade)"));
        let no_load = ImportOptions {
            load: "Heavy",
            ..options()
        };
        assert!(fails(&sample, Some(CONSTANTS), no_load)
            .message
            .contains("it has: Light, Medium"));
        let no_attacks = ImportOptions {
            weapon: "0",
            two_handed: true,
            ..options()
        };
        assert!(fails(&sample, Some(CONSTANTS), no_attacks)
            .message
            .contains("no usable attacks"));
        assert!(
            fails("fn main() {}", Some(CONSTANTS), ImportOptions::default())
                .message
                .contains("`fn base` was not found")
        );

        // A field this reader does not know, a missing one, and a broken record all name a line.
        let line_of = |text: &str, needle: &str| {
            text.lines()
                .position(|line| line.contains(needle))
                .expect(needle)
                + 1
        };
        let backstep = line_of(&sample, "Backstep =>");
        let renamed = sample.replacen("total: 30.0", "length: 30.0", 1);
        let error = fails(&renamed, Some(CONSTANTS), options());
        assert_eq!(
            (error.line, error.message.as_str()),
            (backstep, "unknown `ActionDef` field `length`")
        );
        let missing = sample.replacen("total: 30.0, ", "", 1);
        let error = fails(&missing, Some(CONSTANTS), options());
        assert_eq!(
            (error.line, error.message.as_str()),
            (backstep, "an `ActionDef` has no `total`")
        );
        let broken = sample.replacen("iframes: (0.0, 9.0)", "iframes: (0.0, 9.0", 1);
        let error = fails(&broken, Some(CONSTANTS), options());
        assert_eq!(error.line, backstep, "{error}");
    }
}
