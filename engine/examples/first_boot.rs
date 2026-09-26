//! End-to-end check for Phase 2, with no GUI involved.
//!
//!     cargo run --example first_boot
//!
//! Downloads the emulator, platform-tools and one system image into our own
//! managed SDK folder, creates a device and boots it.

use engine::{avd, downloader, host, licenses, process, repository, sdk_layout};

fn main() -> engine::Result<()> {
    let host = host::detect();
    println!(
        "Machine: {:?} {:?} -> {}",
        host.os,
        host.arch,
        repository::abi_label(host.preferred_abi)
    );

    let sdk = sdk_layout::SdkLayout::create_managed()?;
    println!("SDK folder: {}", sdk.root.display());
    if let Some(free) = host::free_disk_bytes(&sdk.root) {
        println!("Free disk: {} GB", free / 1_073_741_824);
    }

    println!("\nFetching catalog from Google ...");
    let catalog = repository::fetch_catalog()?;
    println!("{} packages available", catalog.packages.len());

    let emulator = catalog
        .latest_tool("emulator", &host)
        .ok_or_else(|| engine::EngineError::NotFound("emulator package".into()))?;
    let platform_tools = catalog
        .latest_tool("platform-tools", &host)
        .ok_or_else(|| engine::EngineError::NotFound("platform-tools package".into()))?;

    // Newest Google Play image this machine can run.
    let image_pkg = catalog
        .system_images(&host)
        .into_iter()
        .find(|p| p.tag_id.as_deref() == Some("google_apis_playstore"))
        .ok_or_else(|| engine::EngineError::NotFound("a compatible system image".into()))?;
    println!(
        "Chosen image: {} ({})",
        image_pkg.friendly_name(),
        image_pkg.path
    );

    // Licences first, exactly like sdkmanager.
    let wanted = vec![emulator, platform_tools, image_pkg];
    for license in licenses::pending(&catalog, &wanted, &sdk) {
        println!("\n--- licence {} (first 200 chars) ---", license.id);
        println!("{}", license.text.chars().take(200).collect::<String>());
        println!("--- accepting for this test run ---");
        // NOTE: auto-accepting here is only correct for this developer test run
        // on our own machine. The shipping app must show the full text and wait
        // for a button press before ever calling licenses::accept.
        licenses::accept(license, &sdk)?;
    }

    for pkg in &wanted {
        println!("\nInstalling {} ...", pkg.path);
        downloader::install_package(pkg, &host, &sdk, |p| match p {
            downloader::Progress::Started { total_bytes } => {
                println!("  {} MB to download", total_bytes / 1_048_576)
            }
            downloader::Progress::Downloading {
                done_bytes,
                total_bytes,
            } => {
                let pct = done_bytes
                    .saturating_mul(100)
                    .checked_div(total_bytes)
                    .unwrap_or(0);
                print!("\r  {pct}% ({} MB)", done_bytes / 1_048_576);
                use std::io::Write;
                let _ = std::io::stdout().flush();
            }
            downloader::Progress::Verifying => println!("\n  verifying checksum"),
            downloader::Progress::Unpacking => println!("  unpacking"),
            downloader::Progress::Finished { .. } => println!("  done"),
        })?;
    }

    let installed = sdk
        .installed_images()
        .into_iter()
        .find(|i| i.package_path == image_pkg.path)
        .ok_or_else(|| engine::EngineError::NotFound("image after install".into()))?;

    let avd_home = sdk_layout::avd_home()
        .ok_or_else(|| engine::EngineError::NotFound("avd home folder".into()))?;
    std::fs::create_dir_all(&avd_home).map_err(|e| engine::io(&avd_home, e))?;

    let name = "AvdHub Test Device";
    if let Ok(existing) = avd::find(&avd_home, &avd::sanitize_name(name)) {
        println!("\nRemoving previous test device");
        avd::delete(&avd_home, &existing)?;
    }

    let device = avd::create(
        &avd_home,
        &sdk,
        name,
        &avd::profiles::default_profile(),
        &installed,
    )?;
    println!("\nCreated {} at {}", device.name, device.dir.display());

    println!("Launching ...");
    let pid = process::launch(&sdk, &device, &process::LaunchOptions::default())?;
    println!("emulator running as pid {pid}");

    println!("Waiting for boot (up to 3 minutes) ...");
    for _ in 0..90 {
        std::thread::sleep(std::time::Duration::from_secs(2));
        if let Ok(list) = process::running(&sdk)
            && let Some(em) = list
                .iter()
                .find(|e| e.avd_name.as_deref() == Some(device.name.as_str()))
            && em.booted
        {
            println!("Booted: {} ({})", em.serial, device.display_name());
            return Ok(());
        }
    }
    println!("Still booting - check the emulator window.");
    Ok(())
}
