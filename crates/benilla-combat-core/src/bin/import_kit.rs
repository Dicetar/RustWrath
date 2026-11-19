//! Build one weapon's combat kit from the owner's locally generated Elden Ring action tables.
//!
//! The input is the generated `extracted.rs` of the combat-rewrite reference (made from the
//! owner's own copy of the game) and the `data.rs` that sits beside it. The output is a kit text
//! file (see `benilla_combat_core::Kit`). Both stay on the owner's machine: write the kit under an
//! ignored directory such as `out/` and never commit it.
//!
//! ```text
//! import_kit --extracted <extracted.rs> --out <kit file>
//!            [--data <data.rs>] [--weapon <name or index>] [--grip one|two]
//!            [--load light|medium|heavy] [--off-hand] [--first-id <n>] [--rate <hz>]
//! ```
//!
//! `--rate` only chooses the tick rate the result is test-compiled at; the kit itself is stored
//! in animation frames and is compiled again by whoever loads it.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{env, fs};

use benilla_combat_core::{import_kit, ActionRevision, ImportOptions, Kit, TickRate};

const USAGE: &str = "usage: import_kit --extracted <extracted.rs> --out <kit file> \
[--data <data.rs>] [--weapon <name or index>] [--grip one|two] [--load light|medium|heavy] \
[--off-hand] [--first-id <n>] [--rate <hz>]";

struct Args {
    extracted: PathBuf,
    data: Option<PathBuf>,
    out: PathBuf,
    weapon: String,
    two_handed: bool,
    load: String,
    off_hand: bool,
    first_id: u32,
    rate_hz: u16,
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    let defaults = ImportOptions::default();
    let mut extracted = None;
    let mut data = None;
    let mut out = None;
    let mut weapon = defaults.weapon.to_owned();
    let mut two_handed = defaults.two_handed;
    let mut load = defaults.load.to_owned();
    let mut off_hand = defaults.off_hand;
    let mut first_id = defaults.first_id;
    let mut rate_hz = 100;

    while let Some(flag) = args.next() {
        if flag == "--off-hand" {
            off_hand = true;
            continue;
        }
        let value = args
            .next()
            .ok_or_else(|| format!("`{flag}` needs a value"))?;
        match flag.as_str() {
            "--extracted" => extracted = Some(PathBuf::from(value)),
            "--data" => data = Some(PathBuf::from(value)),
            "--out" => out = Some(PathBuf::from(value)),
            "--weapon" => weapon = value,
            "--load" => load = value,
            "--grip" => {
                two_handed = match value.as_str() {
                    "one" => false,
                    "two" => true,
                    other => return Err(format!("`--grip` is `one` or `two`, not `{other}`")),
                }
            }
            "--first-id" => {
                first_id = value
                    .parse()
                    .map_err(|_| format!("`--first-id` is a number, not `{value}`"))?;
            }
            "--rate" => {
                rate_hz = value
                    .parse()
                    .map_err(|_| format!("`--rate` is a number, not `{value}`"))?;
            }
            other => return Err(format!("unknown argument `{other}`")),
        }
    }

    Ok(Args {
        extracted: extracted.ok_or("`--extracted` is required")?,
        data,
        out: out.ok_or("`--out` is required")?,
        weapon,
        two_handed,
        load,
        off_hand,
        first_id,
        rate_hz,
    })
}

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("cannot read {}: {error}", path.display()))
}

fn run(args: &Args) -> Result<(), String> {
    let extracted = read(&args.extracted)?;
    // The constants file is taken from beside the generated one unless named; it may be absent
    // when the generated file carries the constants itself.
    let constants = match &args.data {
        Some(path) => Some(read(path)?),
        None => fs::read_to_string(args.extracted.with_file_name("data.rs")).ok(),
    };
    let options = ImportOptions {
        weapon: &args.weapon,
        two_handed: args.two_handed,
        load: &args.load,
        off_hand: args.off_hand,
        first_id: args.first_id,
    };
    let (kit, report) = import_kit(&extracted, constants.as_deref(), &options)
        .map_err(|error| format!("{}: {error}", args.extracted.display()))?;

    // The file is only worth writing if it reads back to the same kit and compiles to a table.
    let text = kit.to_text();
    let reread =
        Kit::parse(&text).map_err(|error| format!("the kit does not read back: {error}"))?;
    if reread != kit {
        return Err("the kit does not read back to itself".to_owned());
    }
    let rate = TickRate::new(args.rate_hz)
        .ok_or_else(|| format!("`--rate` {} is out of range", args.rate_hz))?;
    let table = kit
        .to_table(ActionRevision(1), rate)
        .map_err(|error| format!("the kit does not compile at {} Hz: {error}", args.rate_hz))?;

    if let Some(parent) = args
        .out
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    fs::write(&args.out, &text)
        .map_err(|error| format!("cannot write {}: {error}", args.out.display()))?;

    println!("kit: {}", kit.label);
    println!("weapon: #{} {}", report.weapon_index, report.weapon_name);
    println!(
        "taken: {} dodges, {} attacks ({} actions, {} compiled at {} Hz)",
        report.dodges,
        report.attacks,
        kit.actions.len(),
        table.len(),
        args.rate_hz
    );
    println!(
        "left: {} skipped, {} other base records, {} empty windows dropped",
        report.skipped.len(),
        report.other_base_records,
        report.empty_windows_dropped
    );
    for (name, reason) in &report.skipped {
        println!("  skipped {name}: {reason}");
    }
    println!("wrote {} ({} bytes)", args.out.display(), text.len());
    Ok(())
}

fn main() -> ExitCode {
    let args = match parse_args(env::args().skip(1)) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
