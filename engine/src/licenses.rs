use std::collections::HashSet;
use std::fs;

use sha1::{Digest, Sha1};

use crate::error::{Result, io};
use crate::repository::{Catalog, License, RemotePackage};
use crate::sdk_layout::SdkLayout;

/// The hash sdkmanager stores to record that a licence was accepted.
pub fn license_hash(license: &License) -> String {
    let normalized = license.text.replace("\r\n", "\n");
    let mut hasher = Sha1::new();
    hasher.update(normalized.trim().as_bytes());
    hex::encode(hasher.finalize())
}

/// Licence ids already accepted in this SDK (compatible with Android Studio's files).
pub fn accepted_ids(sdk: &SdkLayout) -> HashSet<String> {
    let mut set = HashSet::new();
    let dir = sdk.root.join("licenses");
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                set.insert(name.to_string());
            }
        }
    }
    set
}

/// Licences the user must still read and accept before these packages install.
pub fn pending<'a>(
    catalog: &'a Catalog,
    packages: &[&RemotePackage],
    sdk: &SdkLayout,
) -> Vec<&'a License> {
    let already = accepted_ids(sdk);
    let mut out: Vec<&License> = Vec::new();
    for pkg in packages {
        if let Some(license) = catalog.license_for(pkg)
            && !already.contains(&license.id)
            && !out.iter().any(|l| l.id == license.id)
        {
            out.push(license);
        }
    }
    out
}

/// Record the user's acceptance the same way sdkmanager does.
///
/// May only ever be called as the direct result of the user pressing an Accept
/// button, with the licence text visible on screen. Never call this on startup
/// or "to save the user a click".
pub fn accept(license: &License, sdk: &SdkLayout) -> Result<()> {
    let dir = sdk.root.join("licenses");
    fs::create_dir_all(&dir).map_err(|e| io(&dir, e))?;
    let file = dir.join(&license.id);
    let contents = format!("\n{}", license_hash(license));
    fs::write(&file, contents).map_err(|e| io(&file, e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accept_writes_hash_file_and_is_detected() {
        let tmp = std::env::temp_dir().join("avdhub-license-test");
        fs::remove_dir_all(&tmp).ok();
        fs::create_dir_all(&tmp).unwrap();
        let sdk = SdkLayout::new(&tmp, true);

        let license = License {
            id: "android-sdk-license".into(),
            text: "Terms and Conditions".into(),
        };
        assert!(accepted_ids(&sdk).is_empty());

        accept(&license, &sdk).unwrap();

        let stored = fs::read_to_string(tmp.join("licenses/android-sdk-license")).unwrap();
        assert!(stored.trim() == license_hash(&license));
        assert!(accepted_ids(&sdk).contains("android-sdk-license"));

        fs::remove_dir_all(&tmp).ok();
    }
}
