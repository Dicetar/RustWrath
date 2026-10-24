//! `wrath-auth`: lab account creation (prints SQL) and live logon against the authserver.
//!
//!   wrath-auth mkacct <account> <password>     prints the INSERT for acore_auth.account
//!   wrath-auth logon <account> <password>      live SRP6 logon + realm list

use anyhow::{Context, Result};
use benilla_protocol_wrath::logon::logon;
use benilla_srp::{generate_account, NormalizedString};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mode = args.next().context("mode required: mkacct | logon")?;
    let account = args.next().context("account required")?;
    let password = args.next().context("password required")?;
    match mode.as_str() {
        "mkacct" => {
            let username_n = NormalizedString::new(&account)?;
            let password_n = NormalizedString::new(&password)?;
            let (salt, verifier) = generate_account(&username_n, &password_n);
            println!(
                "INSERT INTO acore_auth.account (username, salt, verifier, expansion, joindate) \
                 VALUES ('{}', UNHEX('{}'), UNHEX('{}'), 2, NOW());",
                username_n.as_ref(),
                hex(&salt),
                hex(&verifier)
            );
        }
        "logon" => {
            let logon = logon("127.0.0.1:3724", &account, &password)
                .context("logon failed")?;
            println!("authenticated as {}", logon.account);
            println!("session key fingerprint: {}", hex(&logon.session_key[..8]));
            for realm in &logon.realms {
                println!(
                    "realm #{} `{}` type {} flags {:#04x} at {} (pop {:.1}, chars {})",
                    realm.realm_id, realm.name, realm.realm_type, realm.flags,
                    realm.address, realm.population, realm.characters
                );
            }
        }
        other => anyhow::bail!("unknown mode {other}"),
    }
    Ok(())
}
