use engine::{host, repository};

fn main() -> engine::Result<()> {
    let host = host::detect();
    let catalog = repository::fetch_catalog()?;

    let emu = catalog.latest_tool("emulator", &host).unwrap();
    match catalog.license_for(emu) {
        Some(l) => {
            println!("licence id: {}", l.id);
            println!("{} characters", l.text.len());
            println!("\n--- first 600 characters ---\n");
            println!("{}", l.text.chars().take(600).collect::<String>());
        }
        None => println!("no licence attached"),
    }
    Ok(())
}
