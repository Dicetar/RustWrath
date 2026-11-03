//! Read-only provenance/decoder probe for the owner's selected GlueXML profile.
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    anyhow::ensure!(args.len() == 3, "usage: glue_profile DATA MANIFEST PROFILE");
    let mut chain = benilla_assets::open_profile_chain(
        Path::new(&args[0]), Path::new(&args[1]), &args[2],
    )?;
    println!("categories: {:?}", benilla_formats::load_realm_categories(&mut chain, 1));
    for path in [
        r"Interface\Glues\Common\Glue-Tooltip-Border.blp",
        r"Interface\Glues\Common\Glue-Tooltip-Background.blp",
    ] {
        let bytes = chain.read(path)?;
        println!("{path}: {:?}, {} bytes", chain.find_file_archive(path), bytes.len());
        println!("RGBA: {:?}", benilla_formats::blp_to_rgba(&bytes).map(|(w, h, _)| (w, h)));
        println!("render base: {:?}", benilla_formats::blp_to_rgba_base(&bytes).map(|(w, h, _)| (w, h)));
        println!("prefix: {:?}", benilla_formats::blp_bytes_to_native_prefix(&bytes)
            .map(|(image, rejected)| (image.width, image.height, image.mips.len(), rejected)));
    }
    for path in [r"Interface\GlueXML\GlueParent.lua", r"Interface\GlueXML\CharacterCreate.lua"] {
        let bytes = chain.read(path)?;
        println!("{path}: {:?}", chain.find_file_archive(path));
        for (i, line) in String::from_utf8_lossy(&bytes).lines().enumerate() {
            if (path.ends_with("GlueParent.lua") && (330..425).contains(&i))
                || (path.ends_with("CharacterCreate.lua") && (line.contains("GetAvailable") || line.contains("GetSelected") || line.contains("strupper"))) {
                println!("{}: {line}", i + 1);
            }
        }
    }
    for path in [
        r"Interface\Glues\Models\UI_MainMenu_Legion\UI_MainMenu_Legion.m2",
        r"Interface\Glues\Models\UI_Human\UI_Human.m2",
    ] {
        let bytes = chain.read(path)?;
        let cameras = benilla_formats::parse_m2_pane_cameras(&bytes);
        println!("{path}: cameras={}, first={:?}", cameras.len(), cameras.first().map(|c| c.still));
    }
    Ok(())
}
