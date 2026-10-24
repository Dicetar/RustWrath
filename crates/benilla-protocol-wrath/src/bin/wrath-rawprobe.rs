//! `wrath-rawprobe <account> <password>`: logon, world connect, send AUTH_SESSION, then dump
//! every raw byte that arrives (and what our armed crypto makes of it).

use std::io::Read;
use std::time::Duration;

use anyhow::{Context, Result};
use benilla_protocol_wrath::logon::logon;
use benilla_protocol_wrath::opcode;
use benilla_protocol_wrath::world::WorldConnection;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let account = args.next().context("account")?;
    let password = args.next().context("password")?;

    let logon = logon("127.0.0.1:3724", &account, &password)?;
    println!("logon ok, realm_id {}", logon.realms[0].realm_id);
    let (mut world, seed) = WorldConnection::connect("127.0.0.1:8095")?;
    println!("challenge seed {seed:02x?}");
    world.auth_session(&logon.account, &logon.session_key, &seed, logon.realms[0].realm_id as u32)?;
    println!("AUTH_SESSION sent; raw sniffing for 15s...");

    let stream = world.stream_mut();
    stream.set_read_timeout(Some(Duration::from_millis(500)))?;
    let mut total = 0;
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(15) {
        let mut buf = [0u8; 512];
        match stream.read(&mut buf) {
            Ok(0) => {
                println!("EOF at {} bytes", total);
                break;
            }
            Ok(n) => {
                println!("RAW +{}: {:02x?}", n, &buf[..n.min(48)]);
                total += n;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock
                || e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => {
                println!("read error: {e}");
                break;
            }
        }
    }
    println!("total raw bytes after AUTH_SESSION: {total}");
    let _ = opcode::SMSG_AUTH_RESPONSE;
    Ok(())
}
