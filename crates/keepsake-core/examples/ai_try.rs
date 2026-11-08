//! Developer tool: tag image files with the installed model and time it.
//!
//! cargo run --release -p keepsake-core --example ai_try -- [--cpu] <image>...

use std::time::Instant;

use keepsake_core::ai::{dhash, ImageModel, LabelSet};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let cpu = args.first().is_some_and(|a| a == "--cpu");
    if cpu {
        args.remove(0);
    }
    let t = Instant::now();
    let mut model = ImageModel::load_installed(!cpu)?;
    println!("loaded on {:?} in {:?}", model.accel(), t.elapsed());
    let labels = LabelSet::builtin()?;
    let mut total = std::time::Duration::ZERO;
    for path in &args {
        let img = image::open(path)?;
        let t = Instant::now();
        let emb = model.embed(&img)?;
        let dt = t.elapsed();
        total += dt;
        let top: Vec<String> = labels
            .probabilities(&emb)
            .into_iter()
            .take(6)
            .map(|(n, p)| format!("{n} {p:.3}"))
            .collect();
        let name = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().chars().take(12).collect::<String>())
            .unwrap_or_default();
        println!(
            "{name} {:>4} ms dhash {:016x}  {}",
            dt.as_millis(),
            dhash::dhash(&img)?,
            top.join(", ")
        );
    }
    if !args.is_empty() {
        println!("mean {:?}", total / args.len() as u32);
    }
    Ok(())
}
