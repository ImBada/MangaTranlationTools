//! Prints PSD structure (no pixels): `cargo run -p mtt-inpaint --example psd_dump -- FILE.psd`
fn main() {
    let path = std::env::args().nth(1).expect("FILE.psd");
    let bytes = std::fs::read(&path).unwrap();
    match mtt_inpaint::psd::read_psd(&bytes) {
        Ok(doc) => {
            println!("{}x{}, {} leaf layers", doc.width, doc.height, doc.layers.len());
            for layer in &doc.layers {
                let painted = layer.image.pixels().filter(|p| p[3] > 0).count();
                println!(
                    "  {:?} at ({}, {}) {}x{} opacity {:.2} hidden {} painted px {}",
                    layer.name,
                    layer.left,
                    layer.top,
                    layer.image.width(),
                    layer.image.height(),
                    layer.opacity,
                    layer.hidden,
                    painted
                );
            }
        }
        Err(error) => println!("error: {error}"),
    }
}
