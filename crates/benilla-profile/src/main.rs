//! `profile-probe`: resolve, read and classify files across the 3.3.5a profiles.
//!
//! Usage examples (from the repo root, data at the owner's install):
//!   profile-probe status   --data <dir> --manifest <file> --profile base "Maps\Azeroth.wdt"
//!   profile-probe classify --data <dir> --base <file> --hd <file> "Interface\AddOns\WMBridge\WMBridge.lua"
//!   profile-probe read     --data <dir> --manifest <file> --profile wm-hd --out x.adt "Maps\Azeroth\Azeroth_31_48.adt"
//!   profile-probe list     --data <dir> --manifest <file> --profile wm-hd --prefix "Creature\Boar"

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use benilla_profile::{classify, Origin, Profile, Status};

#[derive(Debug)]
struct Args {
    command: String,
    path: Option<String>,
    data: PathBuf,
    manifest: Option<PathBuf>,
    profile: Option<String>,
    base: Option<PathBuf>,
    hd: Option<PathBuf>,
    prefix: Option<String>,
    out: Option<PathBuf>,
}

fn parse_args() -> Result<Args> {
    let mut it = std::env::args().skip(1);
    let command = it.next().context("a command is required: status | classify | read | list")?;
    let mut args = Args {
        command,
        path: None,
        data: PathBuf::from("."),
        manifest: None,
        profile: None,
        base: None,
        hd: None,
        prefix: None,
        out: None,
    };
    let mut next = |flag: &str, it: &mut std::iter::Skip<std::env::Args>| -> Result<String> {
        it.next().with_context(|| format!("{flag} needs a value"))
    };
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--data" => args.data = PathBuf::from(next("--data", &mut it)?),
            "--manifest" => args.manifest = Some(PathBuf::from(next("--manifest", &mut it)?)),
            "--profile" => args.profile = Some(next("--profile", &mut it)?),
            "--base" => args.base = Some(PathBuf::from(next("--base", &mut it)?)),
            "--hd" => args.hd = Some(PathBuf::from(next("--hd", &mut it)?)),
            "--prefix" => args.prefix = Some(next("--prefix", &mut it)?),
            "--out" => args.out = Some(PathBuf::from(next("--out", &mut it)?)),
            other if other.starts_with("--") => bail!("unknown flag {other}"),
            other => {
                if args.path.is_some() { bail!("one path argument at most"); }
                args.path = Some(other.to_string());
            }
        }
    }
    Ok(args)
}

fn origin_tag(origin: Option<Origin>) -> &'static str {
    match origin { Some(Origin::Stock) => "stock", Some(Origin::Wm) => "wm", None => "-" }
}

fn print_status(label: &str, status: &Status, origin: Option<Origin>) {
    match status {
        Status::Found { archive, size } =>
            println!("{label}: found [{:>5}] {} bytes from {}", origin_tag(origin), size, archive.display()),
        Status::Missing => println!("{label}: missing"),
        Status::Tombstoned { archive } =>
            println!("{label}: tombstoned by {}", archive.display()),
    }
}

fn open_profile(args: &Args, manifest: &Option<PathBuf>, profile: &Option<String>) -> Result<Profile> {
    let manifest = manifest.as_ref().context("--manifest is required for this command")?;
    let profile = profile.as_deref().unwrap_or("profile");
    Profile::open(&args.data, manifest, profile)
}

fn main() -> Result<()> {
    let args = parse_args()?;
    match args.command.as_str() {
        "status" => {
            let path = args.path.as_deref().context("status needs an internal path")?;
            let profile = open_profile(&args, &args.manifest, &args.profile)?;
            print_status(profile.profile(), &profile.status(path), profile.origin_of(path));
        }
        "classify" => {
            let path = args.path.as_deref().context("classify needs an internal path")?;
            let base = Profile::open(&args.data, args.base.as_ref().context("--base manifest required")?, "base")?;
            let hd = Profile::open(&args.data, args.hd.as_ref().context("--hd manifest required")?, "wm-hd")?;
            let (class, a, b) = classify(&base, &hd, path);
            print_status("base", &a.status, a.origin);
            print_status("wm-hd", &b.status, b.origin);
            println!("class: {class:?}");
        }
        "read" => {
            let path = args.path.as_deref().context("read needs an internal path")?;
            let out = args.out.as_deref().context("read needs --out")?;
            let profile = open_profile(&args, &args.manifest, &args.profile)?;
            let bytes = profile.read(path)?;
            std::fs::write(out, &bytes).with_context(|| format!("writing {}", out.display()))?;
            println!("{} bytes of {path} written to {}", bytes.len(), out.display());
        }
        "list" => {
            let profile = open_profile(&args, &args.manifest, &args.profile)?;
            let prefix = args.prefix.as_deref().unwrap_or("");
            // Union of every archive's (listfile), filtered by prefix, case-insensitive.
            for (origin, archive) in profile.archives() {
                let Ok(listfile) = archive.read_file("(listfile)") else { continue };
                for raw in String::from_utf8_lossy(&listfile).split([';', '\r', '\n']) {
                    let name = raw.trim();
                    if name.is_empty() { continue; }
                    if prefix.is_empty()
                        || name.len() >= prefix.len()
                            && name[..prefix.len()].eq_ignore_ascii_case(prefix)
                    {
                        let tag = origin_tag(Some(origin));
                        let archive_name = archive
                            .path()
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy();
                        println!("[{tag}] {archive_name}");
                        println!("  {name}");
                    }
                }
            }
        }
        other => bail!("unknown command {other}"),
    }
    Ok(())
}
