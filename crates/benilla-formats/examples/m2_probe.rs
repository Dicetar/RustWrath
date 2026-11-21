//! Parse one M2 the way the actor route does (MD20 + `00.skin`) and print what comes out: every
//! batch's texture and vertex count, or the error that stops the load.
//! `cargo run -p benilla-formats --example m2_probe -- <m2 path>`

fn main() -> anyhow::Result<()> {
    let virt = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: m2_probe <m2 path>"))?;
    let data = benilla_formats::wow_data().expect("no WoW install found (set $WOW_DATA)");
    let mut chain = benilla_formats::open_chain(&data)?;
    let bytes = chain.read_file(&virt)?;
    println!("{virt}: {} bytes, magic {:?}", bytes.len(), &bytes[..4]);
    let skin_path = format!("{}00.skin", virt.trim_end_matches(".m2"));
    let skin_bytes = chain.read_file(&skin_path)?;
    let skin = benilla_formats::parse_external_skin(&skin_bytes)?;
    match benilla_formats::parse_m2_render_submeshes_with_skin(&bytes, "", &[], Some(&skin)) {
        Ok(subs) => {
            println!("{} batches", subs.len());
            for (i, s) in subs.iter().enumerate() {
                println!(
                    "  batch {i:2}: geoset={:?} skin_slot={:?} tex={:?}",
                    s.geoset_id, s.skin_slot, s.texture
                );
            }
        }
        Err(e) => println!("submesh parse FAILED: {e:#}"),
    }
    match benilla_formats::parse_m2_particle_emitters(&bytes) {
        Ok(emitters) => {
            println!("{} emitters", emitters.len());
            for (i, e) in emitters.iter().enumerate() {
                println!(
                    "  emitter {i}: flags={:#x} bone={} tex={:?} blend={:?}",
                    e.flags, e.bone, e.texture, e.blend
                );
            }
        }
        Err(e) => println!("emitter parse FAILED: {e:#}"),
    }
    println!(
        "skeleton: {:?}",
        benilla_formats::parse_m2_skeleton(&bytes).map(|s| s.bones.len())
    );
    println!(
        "attachments: {:?}",
        benilla_formats::parse_m2_attachments(&bytes).map(|a| a.len())
    );
    Ok(())
}
