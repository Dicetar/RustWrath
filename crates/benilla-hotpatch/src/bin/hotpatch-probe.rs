//! Bounded native-loader smoke instrument. This is not rendered-client acceptance.
//!
//! Run it against an isolated module root. It has no lock against a live client: pointing it at
//! the client's Hotpatch folder would let it adopt/clear that client's crash marker and rewrite
//! `quarantine.toml` last-writer-wins.
use benilla_hotpatch::Host;
use benilla_hotpatch_abi::{BoneLocal, RigSpan};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

fn main() -> anyhow::Result<()> {
    let root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("usage: hotpatch-probe MODULE_DIRECTORY [SECONDS]"))?;
    let seconds = std::env::args()
        .nth(2)
        .map(|n| n.parse())
        .transpose()?
        .unwrap_or(30u64);
    anyhow::ensure!(seconds <= 120, "probe deadline must be at most 120 seconds");
    let mut host = Host::new(root);
    // Probe parity with HotpatchPlugin: the dev instrument must exercise the same persistent
    // quarantine and crash-sentinel recovery path as the production adapter instead of starting
    // from empty in-memory stores. Failures are reported and never fail the probe.
    if let Err(error) = host.load_persistent_quarantine() {
        eprintln!("quarantine load: {error:#}");
    }
    match host.load_active_sentinel() {
        Ok(Some(digest)) => {
            eprintln!("recovered crashed module {digest}; quarantining before the probe loop");
            if let Err(error) = host.persist_quarantine() {
                eprintln!("quarantine save after sentinel recovery: {error:#}");
            }
        }
        Ok(None) => {}
        Err(error) => eprintln!("active sentinel load: {error:#}"),
    }
    // Mirror HotpatchPlugin::build: drain a marker-clear failure from the recovery persist
    // immediately, before the loop (and even when the deadline is 0/1 s).
    if let Some(error) = host.take_sentinel_error() {
        eprintln!("{error}");
    }
    let mut last_persist = Instant::now();
    let mut last_quarantine_error: Option<String> = None;
    let mut last_sentinel_error: Option<String> = None;
    let bones = [BoneLocal {
        translation: [0.0; 3],
        rotation: [0.0, 0.0, 0.0, 2.0],
        scale: [1.0; 3],
        parent: -1,
    }];
    let rigs = [RigSpan {
        entity: 1,
        first: 0,
        count: 1,
    }];
    let mut output = Vec::new();
    let mut reported = None;
    let start = Instant::now();
    let mut last_report = Instant::now();
    let mut inject = std::env::args().nth(3).as_deref() == Some("inject-invalid-once");
    println!(
        "PID={} native pose swap probe; deadline={seconds}s",
        std::process::id()
    );
    while start.elapsed() < Duration::from_secs(seconds) {
        let staged = match host.poll() {
            Ok(staged) => staged,
            Err(error) => {
                eprintln!("publication rejected: {error:#}");
                false
            }
        };
        let mut input = bones;
        if staged && host.revision().is_some() && inject {
            input[0].rotation[3] = f32::NAN;
            inject = false;
            eprintln!("injecting one malformed pose frame into the replacement candidate");
        }
        match host.evaluate(&input, &rigs, start.elapsed().as_secs_f64(), &mut output) {
            Ok(true) => {
                let revision = host.revision().map(str::to_owned);
                if revision != reported || last_report.elapsed() >= Duration::from_secs(1) {
                    println!(
                        "PID={} revision={} quaternion_w={}",
                        std::process::id(),
                        revision.as_deref().unwrap_or("none"),
                        output[0].rotation[3]
                    );
                    reported = revision;
                    last_report = Instant::now();
                }
            }
            Ok(false) => {}
            Err(error) => eprintln!("frame rejected: {error:#}"),
        }
        if let Some(error) = host.take_rejection() {
            eprintln!("{error}");
        }
        if let Err(error) = host.collect_retired() {
            eprintln!("artifact retention: {error:#}");
        }
        if last_persist.elapsed() >= Duration::from_secs(1) {
            match host.persist_quarantine() {
                Ok(()) => last_quarantine_error = None,
                Err(error) => {
                    let message = format!("quarantine save: {error:#}");
                    if last_quarantine_error.as_deref() != Some(&message) {
                        eprintln!("{message}");
                    }
                    last_quarantine_error = Some(message);
                }
            }
            if let Some(error) = host.take_sentinel_error() {
                if last_sentinel_error.as_deref() != Some(&error) {
                    eprintln!("{error}");
                }
                last_sentinel_error = Some(error);
            }
            last_persist = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    // Final flush so a digest or sentinel error from the last sub-second is not lost. The probe
    // remains a dev instrument: this is best-effort and never fails the run.
    if let Err(error) = host.persist_quarantine() {
        eprintln!("quarantine save: {error:#}");
    }
    if let Some(error) = host.take_sentinel_error() {
        eprintln!("{error}");
    }
    Ok(())
}
