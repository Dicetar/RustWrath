//! Bounded native-loader smoke instrument. This is not rendered-client acceptance.
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
        host.collect_retired()?;
        std::thread::sleep(Duration::from_millis(250));
    }
    Ok(())
}
