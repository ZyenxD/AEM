use engine::host;

fn main() {
    let info = host::detect();
    println!("{info:#?}");
    println!("manifest os   = {}", info.os.manifest_name());
    println!("manifest arch = {}", info.arch.manifest_name());
    println!("image abi     = {}", info.preferred_abi.manifest_name());
    println!(
        "RAM           = {} GB",
        info.total_ram_bytes / 1_073_741_824
    );

    let home = dirs::home_dir().unwrap();
    println!("free on home  = {:?} bytes", host::free_disk_bytes(&home));
}
