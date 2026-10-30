//! Inspect a local BLP's strict result and rendering fallback without opening the client.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: inspect_mips <file.blp>")?;
    let bytes = std::fs::read(path)?;
    match benilla_blp::decode_native(&bytes) {
        Ok(image) => println!("strict: {} levels", image.mips.len()),
        Err(error) => println!("strict: {error}"),
    }
    let (image, rejected) = benilla_blp::decode_native_with_mip_fallback(&bytes)?;
    println!("render: {}x{}, {} levels, rejected={rejected:?}", image.width, image.height, image.mips.len());
    for (level, mip) in image.mips.iter().enumerate() {
        assert_eq!(mip.bytes.len(), image.texels.level_bytes(mip.width, mip.height));
        println!("{level}: {}x{}, {} bytes", mip.width, mip.height, mip.bytes.len());
    }
    Ok(())
}
