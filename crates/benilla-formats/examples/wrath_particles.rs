//! Inspect validated v264 particle records from a local extracted model. No client execution.
use benilla_formats::wrath_particles::parse_wrath_particle_records;

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: wrath_particles <model.m2>"))?;
    let bytes = std::fs::read(path)?;
    let emitters = parse_wrath_particle_records(&bytes)?;
    println!("validated {} v264 particle records", emitters.len());
    for emitter in emitters {
        println!("id={} shape={} flags={:#x} texture={} atlas={}x{} color/alpha/size keys={}/{}/{} rate sequences={} lifeVariation={} rateVariation={}",
            emitter.id, emitter.shape, emitter.flags, emitter.texture_index, emitter.rows, emitter.columns,
            emitter.color.keys.len(), emitter.alpha.keys.len(), emitter.scale.keys.len(),
            emitter.tracks[6].sequences.len(), emitter.life_variation, emitter.emission_variation);
    }
    match benilla_formats::parse_m2_particle_emitters(&bytes) {
        Ok(emitters) => {
            println!("runtime supports {} emitters", emitters.len());
            for emitter in emitters {
                println!(
                    "rate={} lifespan={} age-mid={:?}",
                    emitter.timing.rate(Some(0), 0.0, 0.0),
                    emitter.params.sample(Some(0), 0.0, 0.0).lifespan,
                    emitter.sample_over_life(0.5)
                );
            }
        }
        Err(error) => println!("runtime guard retained: {error:#}"),
    }
    Ok(())
}
