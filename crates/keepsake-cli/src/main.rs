use anyhow::Result;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("keepsake {}", keepsake_core::version());
        return Ok(());
    }

    println!("keepsake {}", keepsake_core::version());
    println!("Run `keepsake --help` for available commands.");
    Ok(())
}
