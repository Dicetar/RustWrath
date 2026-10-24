"""Patch: wire SessionState into the wrath-world driver (the application boundary)."""
p = "crates/benilla-protocol-wrath/src/bin/wrath-world.rs"
s = open(p, encoding="utf-8").read()

# --- imports + clock helpers ---
old = """use benilla_protocol_wrath::logon::logon;
use benilla_protocol_wrath::opcode;
use benilla_protocol_wrath::world::{is_idle, WorldConnection};"""
new = """use benilla_protocol_wrath::logon::logon;
use benilla_protocol_wrath::opcode;
use benilla_protocol_wrath::session::SessionState;
use benilla_protocol_wrath::world::{is_idle, WorldConnection};

/// Driver-owned clocks (the producer stays time-free): wall-clock for observations and
/// monotonic session uptime.
fn unix_now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}"""
assert old in s, "import anchor missing"
s = s.replace(old, new, 1)

# --- create the producer after the character identity is known ---
old = """    let guid = guid.context("no character to log in")?;
    world.send(opcode::CMSG_PLAYER_LOGIN, &guid.to_le_bytes())?;
    println!("[world] PLAYER_LOGIN guid {guid}");"""
new = """    let guid = guid.context("no character to log in")?;

    // --- WM observation producer (convergence task): session-owned replicated state ---
    // The producer reduces every post-login packet; export happens on state change in the
    // dwell loop below. Clocks are driver-owned; the producer is time-free.
    let session_start = Instant::now();
    let mut session = SessionState::new();
    session.start_session(guid);
    let mut exports: u64 = 0;

    world.send(opcode::CMSG_PLAYER_LOGIN, &guid.to_le_bytes())?;
    println!("[world] PLAYER_LOGIN guid {guid}");"""
assert old in s, "producer anchor missing"
s = s.replace(old, new, 1)

# --- dwell loop: reduce, lifecycle, export ---
old = """        match world.recv() {
            Ok(p) => {
                match p.opcode {
                    0x236 => logged_in = true,
                    0x0A9 => saw_update = true,
                    0x390 => {
                        let mut cur = Cur { p: &p.payload, at: 0 };
                        let counter = cur.u32()?;
                        let mut resp = counter.to_le_bytes().to_vec();
                        resp.extend_from_slice(&0u32.to_le_bytes());
                        world.send(CMSG_TIME_SYNC_RESP, &resp)?;
                    }
                    _ => {}
                }
                if std::env::var("WRATH_DEBUG").is_ok() {
                    println!("[dwell] {} ({} bytes)", name_of(p.opcode), p.payload.len());
                }
            }
            Err(e) if is_idle(&e) => {}
            Err(e) => return Err(e),
        }
    }
    if !logged_in {
        bail!("dwell finished without SMSG_LOGIN_VERIFY_WORLD");
    }
    println!("[world] dwell ok: LOGIN_VERIFY_WORLD seen, object updates: {saw_update}");"""
new = """        match world.recv() {
            Ok(p) => {
                let arrival = unix_now_ms();
                session.reduce(p.opcode, &p.payload, arrival)?;
                match p.opcode {
                    0x236 => logged_in = true,
                    0x0A9 => saw_update = true,
                    0x390 => {
                        let mut cur = Cur { p: &p.payload, at: 0 };
                        let counter = cur.u32()?;
                        let mut resp = counter.to_le_bytes().to_vec();
                        resp.extend_from_slice(&0u32.to_le_bytes());
                        world.send(CMSG_TIME_SYNC_RESP, &resp)?;
                    }
                    _ => {}
                }
                if std::env::var("WRATH_DEBUG").is_ok() {
                    println!("[dwell] {} ({} bytes)", name_of(p.opcode), p.payload.len());
                }
            }
            Err(e) if is_idle(&e) => {}
            Err(e) => return Err(e),
        }
        // Lifecycle: a map transition retires the WM publisher (bridge contract) — logged
        // here; a live publisher would be closed and respawned before the next export.
        for event in session.take_lifecycle() {
            println!("[wm] lifecycle {event:?}: retire the WM publisher");
        }
        // Export on source-observed change; the producer retains until mark_published, so a
        // busy/failed handoff is retried by the next loop iteration without losing state.
        if session.export_due() {
            if let Some(snapshot) = session.export(unix_now_ms(), session_start.elapsed().as_millis() as u64)? {
                exports += 1;
                println!(
                    "[wm] seq {} rev {} map {} pos ({:.1}, {:.1}, {:.1}, {:.1}) target {:?} visible {}/{} complete={} age_ms {}",
                    snapshot.sequence,
                    snapshot.state_revision,
                    snapshot.map_id,
                    snapshot.position.x,
                    snapshot.position.y,
                    snapshot.position.z,
                    snapshot.position.o,
                    match &snapshot.target {
                        benilla_protocol_wrath::session::SelfTarget::Unknown => "unknown".to_string(),
                        benilla_protocol_wrath::session::SelfTarget::None => "none".to_string(),
                        benilla_protocol_wrath::session::SelfTarget::Target(g) => format!("{g}"),
                    },
                    snapshot.visibility.guids.len(),
                    snapshot.visibility.total_guid_count,
                    snapshot.visibility.complete,
                    unix_now_ms() - snapshot.observed_at_unix_ms,
                );
                // INTEGRATION POINT: replace this mark with
                //   publisher.publish(&map_to_observation(&snapshot, exported_at_unix_ms))?
                //   -> on Ok: session.mark_published(); on PublisherBusy: nothing (retry);
                //   on map transition / reconnect: retire the publisher first (see
                //   docs/convergence/glm-producer.md).
                session.mark_published();
            }
        }
    }
    if !logged_in {
        bail!("dwell finished without SMSG_LOGIN_VERIFY_WORLD");
    }
    println!("[world] dwell ok: LOGIN_VERIFY_WORLD seen, object updates: {saw_update}, wm exports: {exports}");"""
assert old in s, "dwell anchor missing"
s = s.replace(old, new, 1)

open(p, "w", encoding="utf-8", newline="\n").write(s)
print("driver wired")
