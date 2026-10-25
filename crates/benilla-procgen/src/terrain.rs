//! Seeded terrain fields: hash-lattice value noise raised to FBM, a second independent
//! moisture field, and a height×moisture biome grid. Output is a plain data field —
//! rendering, tiling into ADT chunks, and server collision/navigation are consumer seams.

use crate::rng::ProcRng;
use crate::ValidationError;

/// The fork domain for the terrain stream.
pub const DOMAIN: u64 = 0x5445_5252_4149_0003; // "TERRAIN"

/// Biome classification from (height, moisture).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Biome {
    Ocean,
    Beach,
    Desert,
    Plains,
    Forest,
    Hills,
    Mountain,
    Snow,
}

impl Biome {
    pub fn discriminant_byte(self) -> u8 {
        match self {
            Biome::Ocean => 1,
            Biome::Beach => 2,
            Biome::Desert => 3,
            Biome::Plains => 4,
            Biome::Forest => 5,
            Biome::Hills => 6,
            Biome::Mountain => 7,
            Biome::Snow => 8,
        }
    }
}

/// Generation parameters.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainSpec {
    /// Field dimensions in samples (not world units; the consumer maps scale).
    pub width: u32,
    pub height: u32,
    /// Base lattice frequency (features across the field's long axis).
    pub frequency: f32,
    /// FBM octaves.
    pub octaves: u32,
    /// Height below which the field is ocean.
    pub sea_level: f32,
}

impl Default for TerrainSpec {
    fn default() -> Self {
        Self { width: 128, height: 128, frequency: 4.0, octaves: 5, sea_level: 0.35 }
    }
}

impl TerrainSpec {
    /// Reject invalid samples, non-finite noise parameters and excessive generation work.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.width == 0 || self.height == 0 || self.width > 2048 || self.height > 2048
            || u64::from(self.width) * u64::from(self.height) > 1_048_576
        {
            return Err(ValidationError::new("terrain.dimensions", "nonzero sides up to 2048, at most 1048576 samples"));
        }
        if !self.frequency.is_finite() || !(0.0..=1024.0).contains(&self.frequency) {
            return Err(ValidationError::new("terrain.frequency", "expected finite 0..=1024"));
        }
        if !(1..=16).contains(&self.octaves) {
            return Err(ValidationError::new("terrain.octaves", "expected 1..=16 octaves"));
        }
        if !self.sea_level.is_finite() || !(0.0..=1.0).contains(&self.sea_level) {
            return Err(ValidationError::new("terrain.sea_level", "expected finite 0..=1"));
        }
        Ok(())
    }
}

/// The generated field. Heights and moisture are normalized to `[0, 1]`.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainField {
    pub width: u32,
    pub height: u32,
    pub heights: Vec<f32>,
    pub moisture: Vec<f32>,
    pub biomes: Vec<Biome>,
}

impl TerrainField {
    pub fn height_at(&self, x: u32, y: u32) -> f32 {
        self.heights[(y * self.width + x) as usize]
    }

    pub fn biome_at(&self, x: u32, y: u32) -> Biome {
        self.biomes[(y * self.width + x) as usize]
    }
}

/// Generate the field from `(rng, spec)`. The rng contributes only a seed offset per layer;
/// all spatial structure is a pure function of lattice coordinates and that offset, so the
/// field is stable and tile-friendly.
pub fn generate(rng: &mut ProcRng, spec: &TerrainSpec) -> Result<TerrainField, ValidationError> {
    spec.validate()?;
    let height_seed = rng.next_u64();
    let moisture_seed = rng.next_u64();
    let n = (spec.width * spec.height) as usize;

    let mut heights = Vec::with_capacity(n);
    let mut max_h = f32::MIN;
    let mut min_h = f32::MAX;
    for y in 0..spec.height {
        for x in 0..spec.width {
            let fx = x as f32 / spec.width.max(1) as f32;
            let fy = y as f32 / spec.height.max(1) as f32;
            let h = fbm(fx * spec.frequency, fy * spec.frequency, height_seed, spec.octaves);
            heights.push(h);
            max_h = max_h.max(h);
            min_h = min_h.min(h);
        }
    }
    let span = (max_h - min_h).max(f32::EPSILON);
    for h in &mut heights {
        *h = (*h - min_h) / span;
    }

    let mut moisture = Vec::with_capacity(n);
    let mut max_m = f32::MIN;
    let mut min_m = f32::MAX;
    for y in 0..spec.height {
        for x in 0..spec.width {
            let fx = x as f32 / spec.width.max(1) as f32;
            let fy = y as f32 / spec.height.max(1) as f32;
            // Moisture: different frequency so the two fields de-correlate.
            let m = fbm(fx * spec.frequency * 0.7 + 3.1, fy * spec.frequency * 0.7 + 7.7, moisture_seed, spec.octaves);
            moisture.push(m);
            max_m = max_m.max(m);
            min_m = min_m.min(m);
        }
    }
    let span_m = (max_m - min_m).max(f32::EPSILON);
    for m in &mut moisture {
        *m = (*m - min_m) / span_m;
    }

    let biomes = heights
        .iter()
        .zip(&moisture)
        .map(|(&h, &m)| classify(h, m, spec.sea_level))
        .collect();

    Ok(TerrainField { width: spec.width, height: spec.height, heights, moisture, biomes })
}

fn classify(h: f32, m: f32, sea_level: f32) -> Biome {
    if h < sea_level {
        Biome::Ocean
    } else if h < sea_level + 0.05 {
        Biome::Beach
    } else if h > 0.9 {
        Biome::Snow
    } else if h > 0.78 {
        Biome::Mountain
    } else if h > 0.6 {
        Biome::Hills
    } else if m < 0.3 {
        Biome::Desert
    } else if m > 0.6 {
        Biome::Forest
    } else {
        Biome::Plains
    }
}

/// Fractal Brownian motion over value noise: `octaves` layers, gain 0.5, lacunarity 2.
fn fbm(x: f32, y: f32, seed: u64, octaves: u32) -> f32 {
    let mut sum = 0.0;
    let mut amplitude = 1.0;
    let mut total = 0.0;
    let mut fx = x;
    let mut fy = y;
    for octave in 0..octaves {
        sum += value_noise(fx, fy, seed ^ (octave as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)) * amplitude;
        total += amplitude;
        amplitude *= 0.5;
        fx *= 2.0;
        fy *= 2.0;
    }
    sum / total
}

/// Smooth-interpolated hash-lattice value noise in `[0, 1]`.
fn value_noise(x: f32, y: f32, seed: u64) -> f32 {
    let x0 = x.floor();
    let y0 = y.floor();
    let sx = smooth(x - x0);
    let sy = smooth(y - y0);
    let x1 = x0 + 1.0;
    let y1 = y0 + 1.0;
    let v00 = lattice(x0 as i32, y0 as i32, seed);
    let v10 = lattice(x1 as i32, y0 as i32, seed);
    let v01 = lattice(x0 as i32, y1 as i32, seed);
    let v11 = lattice(x1 as i32, y1 as i32, seed);
    let top = v00 + (v10 - v00) * sx;
    let bottom = v01 + (v11 - v01) * sx;
    top + (bottom - top) * sy
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Hash one lattice point into `[0, 1)` — SplitMix-style mixing of the coordinates.
fn lattice(x: i32, y: i32, seed: u64) -> f32 {
    let mut z = seed
        ^ (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    let mixed = z ^ (z >> 31);
    (mixed >> 40) as f32 / 16_777_216.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_spec_same_field() {
        let mut a = ProcRng::new(5);
        let mut b = ProcRng::new(5);
        assert_eq!(
            generate(&mut a, &TerrainSpec::default()).expect("valid terrain"),
            generate(&mut b, &TerrainSpec::default()).expect("valid terrain"),
        );
    }

    #[test]
    fn normalized_and_biomed() {
        let field = generate(&mut ProcRng::new(9), &TerrainSpec::default()).expect("valid terrain");
        assert!(field.heights.iter().all(|h| (0.0..=1.0001).contains(h)));
        assert!(field.moisture.iter().all(|m| (0.0..=1.0001).contains(m)));
        assert_eq!(field.biomes.len(), field.heights.len());
    }

    #[test]
    fn a_large_field_has_biome_variety() {
        let field = generate(&mut ProcRng::new(13), &TerrainSpec { width: 256, height: 256, ..Default::default() }).expect("valid terrain");
        let mut kinds = std::collections::HashSet::new();
        for biome in &field.biomes {
            kinds.insert(*biome);
        }
        // At least five distinct biomes on a 256² field with default thresholds.
        assert!(kinds.len() >= 5, "only {} biomes: {:?}", kinds.len(), kinds);
    }
}
