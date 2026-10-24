//! Dump the first raw bytes of a fresh world connection (before any interpretation).
use std::io::Read;
use std::net::TcpStream;
use std::time::Duration;

fn main() -> std::io::Result<()> {
    let mut s = TcpStream::connect("127.0.0.1:8095")?;
    s.set_read_timeout(Some(Duration::from_secs(3)))?;
    let mut buf = [0u8; 96];
    let mut got = 0;
    while got < buf.len() {
        match s.read(&mut buf[got..]) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(_) => break,
        }
    }
    println!("got {got} bytes:");
    for (i, chunk) in buf[..got].chunks(16).enumerate() {
        println!("{:04x}: {:02x?}", i * 16, chunk);
    }
    Ok(())
}
