//! Real-file light-table diagnostic. Never commit the input game asset.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: light_sources MODEL.m2")?;
    let bytes = std::fs::read(path)?;
    if bytes.get(4..8) != Some(&264u32.to_le_bytes()) { return Err("expected v264 M2".into()); }
    let declared = u32::from_le_bytes(bytes.get(0x108..0x10c).ok_or("short header")?.try_into()?);
    let lights = benilla_formats::parse_m2_lights(&bytes);
    println!("Wrath light table declared={declared} decoded={}", lights.len());
    if lights.len() != declared as usize { return Err("light parser decoded a different table".into()); }
    Ok(())
}
