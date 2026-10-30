//! Inspect decoded liquid heights in a local ADT without launching a client.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: liquid_heights <file.adt>")?;
    let bytes = std::fs::read(path)?;
    let benilla_adt::ParsedAdt::Root(root) =
        benilla_adt::parse_adt(&mut std::io::Cursor::new(bytes.as_slice()))?;
    let mut layers = 0;
    for entry in root.mh2o.iter().flat_map(|chunk| &chunk.entries) {
        for layer in &entry.instances {
            layers += 1;
            let cols = usize::from(layer.width) + 1;
            let mut used = vec![false; layer.heights.len()];
            for y in 0..usize::from(layer.height) {
                for x in 0..usize::from(layer.width) {
                    let cell = y * usize::from(layer.width) + x;
                    if layer.exists.is_some_and(|mask| mask >> cell & 1 == 0) { continue; }
                    for vertex in [y * cols + x, y * cols + x + 1, (y + 1) * cols + x, (y + 1) * cols + x + 1] {
                        if let Some(value) = used.get_mut(vertex) { *value = true; }
                    }
                }
            }
            for (&height, used) in layer.heights.iter().zip(used) {
                if !used { continue; }
                if !height.is_finite() || height < layer.min_height - 0.1 || height > layer.max_height + 0.1 {
                    return Err(format!("height {height} outside authored bounds {}..{}", layer.min_height, layer.max_height).into());
                }
            }
        }
    }
    println!("validated {layers} MH2O layers against authored height bounds");
    Ok(())
}
