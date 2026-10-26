//! Explicit offline file adapter. It never invokes WM, a client, a shell or a server.

use std::error::Error;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use benilla_procgen::artifact::{BuildIdentity, Candidate, Request};
use benilla_procgen::planning::{EncounterPlan, PlacementRequest};
use benilla_procgen::wm_draft::{ApplicationReceipt, CleanupEvidence, DraftOptions, WmDraft};
use benilla_procgen::json;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const USAGE: &str = "procgen-offline --workspace-root ROOT COMMAND INPUTS OUTPUT_NAME.json\n\
commands (inputs in order):\n\
  generate REQUEST BUILD\n\
  inspect CANDIDATE BUILD\n\
  plan CANDIDATE BUILD PLACEMENT_REQUEST\n\
  draft CANDIDATE BUILD PLAN OPTIONS\n\
  receipt CANDIDATE BUILD PLAN DRAFT WM_REPORT\n\
  cleanup CANDIDATE BUILD PLAN DRAFT WM_REPORT RECEIPT\n\
  cleanup-receipt CANDIDATE BUILD PLAN DRAFT WM_REPORT RECEIPT CLEANUP_REPORT CLEANUP_RUN\n\
Output is create-new only under ROOT/benilla-config/procgen/offline. No live operations.";

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CleanupRun { run_key: String }

fn read(path: &OsString) -> Result<Vec<u8>> {
    let file = File::open(Path::new(path))?;
    let metadata = file.metadata()?;
    if !metadata.is_file() { return Err("input must be a regular file".into()); }
    if metadata.len() > json::MAX_BYTES as u64 {
        return Err("input exceeds 64 MiB".into());
    }
    let mut bytes = Vec::new();
    file.take(json::MAX_BYTES as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > json::MAX_BYTES { return Err("input grew beyond 64 MiB".into()); }
    Ok(bytes)
}

fn output_directory(root: &Path) -> Result<PathBuf> {
    let root = root.canonicalize()?;
    if !root.join("Cargo.toml").is_file() || !root.join("crates/benilla-procgen/Cargo.toml").is_file() {
        return Err("workspace root must contain the Project B procgen workspace".into());
    }
    let mut directory = root.clone();
    for part in ["benilla-config", "procgen", "offline"] {
        directory.push(part);
        match fs::symlink_metadata(&directory) {
            Ok(_) => {
                let canonical = directory.canonicalize()?;
                if !canonical.starts_with(&root) || !canonical.is_dir() {
                    return Err("output path escapes workspace or is not a directory".into());
                }
                directory = canonical;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&directory)?,
            Err(e) => return Err(e.into()),
        }
    }
    Ok(directory)
}

fn save(root: &Path, name: &OsString, bytes: &[u8]) -> Result<PathBuf> {
    let name = name.to_str().ok_or("output filename must be UTF-8")?;
    if name.len() > 128 || !name.ends_with(".json") || name.len() <= 5
        || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
        || name.contains("..") {
        return Err("output requires a plain bounded .json filename".into());
    }
    let base = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    if matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (base.len() == 4 && (base.starts_with("COM") || base.starts_with("LPT"))
            && matches!(base.as_bytes()[3], b'1'..=b'9')) {
        return Err("reserved Windows device filename refused".into());
    }
    let path = output_directory(root)?.join(name);
    // Refuse overwrite, including an existing symlink. No destination is opened until
    // the complete source operation and bounded encoding have succeeded.
    let mut file = OpenOptions::new().write(true).create_new(true).open(&path)?;
    if let Err(e) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(&path);
        return Err(e.into());
    }
    Ok(path)
}

fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).take(13).collect();
    if args.len() == 1 && args[0] == "--help" { println!("{USAGE}"); return Ok(()); }
    if args.len() < 6 || args.len() > 12 || args[0] != "--workspace-root" {
        return Err(USAGE.into());
    }
    let command = args[2].to_str().ok_or("command must be UTF-8")?;
    let count = match command {
        "generate" | "inspect" => 2,
        "plan" => 3,
        "draft" => 4,
        "receipt" => 5,
        "cleanup" => 6,
        "cleanup-receipt" => 8,
        _ => return Err(USAGE.into()),
    };
    if args.len() != count + 4 { return Err(USAGE.into()); }
    // Read and validate every input before creating any output directories.
    let inputs = args[3..3 + count].iter().map(read).collect::<Result<Vec<_>>>()?;
    let build: BuildIdentity = json::decode(&inputs[1])?;
    build.validate()?;
    let bytes = if command == "generate" {
        let request: Request = json::decode(&inputs[0])?;
        let spec = request.spec();
        let generated = benilla_procgen::generate(&spec)?;
        Candidate::capture(&spec, &generated, build.clone())?.encode(&build)?
    } else {
        let candidate = Candidate::decode(&inputs[0], &build)?;
        match command {
            "inspect" => json::encode(&serde_json::json!({
                "schema_version": "project-b.procgen.inspection.v1",
                "candidate_manifest": candidate.manifest_hash,
                "generator_version": candidate.generator_version,
                "source_identity": candidate.source_identity,
                "items": candidate.items.len(), "rooms": candidate.dungeon.rooms.len(),
                "terrain_samples": candidate.terrain.heights.len(), "actors": candidate.spawns.len(),
                "data_consistent": true, "requires_owner_preflight": true,
                "runtime_acceptance": "UNKNOWN"
            }))?,
            "plan" => {
                let request = PlacementRequest::decode(&inputs[2])?;
                EncounterPlan::build(&candidate, &build, request)?.encode(&candidate, &build)?
            }
            "draft" => {
                let plan = EncounterPlan::decode(&inputs[2], &candidate, &build)?;
                let options: DraftOptions = json::decode(&inputs[3])?;
                WmDraft::build(&plan, &candidate, &build, options)?.encode(&plan, &candidate, &build)?
            }
            "receipt" | "cleanup" | "cleanup-receipt" => {
                let plan = EncounterPlan::decode(&inputs[2], &candidate, &build)?;
                let draft = WmDraft::decode(&inputs[3], &plan, &candidate, &build)?;
                if command == "receipt" {
                    draft.import_report(&inputs[4], &plan, &candidate, &build)?
                        .encode(&inputs[4], &draft, &plan, &candidate, &build)?
                } else {
                    let receipt = ApplicationReceipt::decode(&inputs[5], &inputs[4], &draft, &plan, &candidate, &build)?;
                    if command == "cleanup" {
                        json::encode(&receipt.cleanup_draft(&inputs[4], &draft, &plan, &candidate, &build)?)?
                    } else {
                        let cleanup_run: CleanupRun = json::decode(&inputs[7])?;
                        let cleanup = receipt.import_cleanup_report(&inputs[4], &draft, &plan, &candidate,
                            &build, &inputs[6], &cleanup_run.run_key)?;
                        cleanup.encode(&CleanupEvidence { application: &receipt, original_report: &inputs[4],
                            draft: &draft, plan: &plan, candidate: &candidate, build: &build,
                            cleanup_report: &inputs[6], cleanup_run_key: &cleanup_run.run_key })?
                    }
                }
            }
            _ => return Err(USAGE.into()),
        }
    };
    let path = save(Path::new(&args[1]), &args[3 + count], &bytes)?;
    println!("{}", path.display());
    Ok(())
}

fn main() {
    if let Err(error) = run() { eprintln!("{error}"); std::process::exit(1); }
}
