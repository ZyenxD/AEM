use engine::{host, repository};

fn main() -> engine::Result<()> {
    let host = host::detect();
    println!("Fetching catalog ...");
    let catalog = repository::fetch_catalog()?;

    println!(
        "{} packages, {} licences",
        catalog.packages.len(),
        catalog.licenses.len()
    );

    if let Some(emu) = catalog.latest_tool("emulator", &host) {
        let a = emu.archive_for(&host).unwrap();
        println!(
            "\nemulator {:?} — {} MB",
            emu.revision,
            a.size_bytes / 1_048_576
        );
        println!("  {}", a.url);
    }

    println!("\nImages for this machine:");
    for pkg in catalog.system_images(&host).iter().take(10) {
        let mb = pkg
            .archive_for(&host)
            .map(|a| a.size_bytes / 1_048_576)
            .unwrap_or(0);
        println!("  [{mb:>5} MB] {:<28} {}", pkg.friendly_name(), pkg.path);
    }
    Ok(())
}
