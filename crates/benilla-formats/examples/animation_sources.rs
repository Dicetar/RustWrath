//! Narrow real-file animation diagnostic. Assets remain outside the repository.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: animation_sources MODEL.m2 TALK.anim")?;
    let bytes = std::fs::read(path)?;
    let external = std::fs::read(std::env::args().nth(2).ok_or("missing TALK.anim")?)?;
    let animations = benilla_formats::parse_m2_animations_with_external(&bytes, |id, variation| {
        (id == 60 && variation == 0).then(|| external.clone())
    });
    let talk = animations.iter().find(|a| a.anim_id == 60).ok_or("talk sequence unavailable")?;
    let bone = talk.bones.iter().find(|b| b.bone == 39).ok_or("bone 39 has no talk keys")?;
    let invalid = bone.translation.iter().flat_map(|(_, v)| v).filter(|v| !v.is_finite()).count();
    println!("talk slot={} bone=39 translation_keys={} nonfinite={invalid}", talk.seq_index, bone.translation.len());
    let invalid_all = talk.bones.iter().map(|bone| {
        bone.translation.iter().chain(&bone.scale).flat_map(|(_, v)| v)
            .filter(|v| !v.is_finite()).count()
            + bone.rotation.iter().flat_map(|(_, v)| v).filter(|v| !v.is_finite()).count()
    }).sum::<usize>();
    println!("talk animated_bones={} all_channels_nonfinite={invalid_all}", talk.bones.len());
    if invalid_all != 0 { return Err("corrupt talking-animation keys reached the production reader".into()); }
    Ok(())
}
