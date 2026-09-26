use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Result, io};

/// A `key=value` file (config.ini and the AVD pointer .ini use the same shape).
///
/// Every key is kept, including ones we do not understand, so editing a device
/// created by Android Studio never destroys its settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IniFile {
    pub entries: BTreeMap<String, String>,
}

impl IniFile {
    pub fn parse(text: &str) -> IniFile {
        let mut entries = BTreeMap::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                entries.insert(key.trim().to_string(), value.trim().to_string());
            }
        }
        IniFile { entries }
    }

    pub fn read(path: &Path) -> Result<IniFile> {
        let text = fs::read_to_string(path).map_err(|e| io(path, e))?;
        Ok(IniFile::parse(&text))
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| io(parent, e))?;
        }
        fs::write(path, self.to_string()).map_err(|e| io(path, e))
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(|s| s.as_str())
    }

    pub fn get_u32(&self, key: &str) -> Option<u32> {
        self.get(key).and_then(|v| v.parse().ok())
    }

    /// Values like "2048M" or "512" used by hw.ramSize / vm.heapSize.
    pub fn get_size_mb(&self, key: &str) -> Option<u32> {
        let raw = self.get(key)?;
        let digits: String = raw.chars().take_while(|c| c.is_ascii_digit()).collect();
        let number: u32 = digits.parse().ok()?;
        match raw.chars().last() {
            Some('G') | Some('g') => Some(number * 1024),
            Some('K') | Some('k') => Some(number / 1024),
            _ => Some(number),
        }
    }

    pub fn set(&mut self, key: &str, value: impl Into<String>) -> &mut Self {
        self.entries.insert(key.to_string(), value.into());
        self
    }
}

impl std::fmt::Display for IniFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (key, value) in &self.entries {
            writeln!(f, "{key}={value}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_unknown_keys() {
        let original = "\
hw.ramSize=2048
# a comment
some.future.key=keep-me

hw.lcd.density=440
";
        let mut ini = IniFile::parse(original);
        assert_eq!(ini.get_size_mb("hw.ramSize"), Some(2048));
        assert_eq!(ini.get("some.future.key"), Some("keep-me"));

        ini.set("hw.ramSize", "4096");
        let rendered = ini.to_string();

        assert!(rendered.contains("hw.ramSize=4096"));
        assert!(
            rendered.contains("some.future.key=keep-me"),
            "unknown keys must survive an edit"
        );
        assert!(rendered.contains("hw.lcd.density=440"));
    }

    #[test]
    fn parses_size_suffixes() {
        let ini = IniFile::parse("a=512M\nb=2G\nc=1024");
        assert_eq!(ini.get_size_mb("a"), Some(512));
        assert_eq!(ini.get_size_mb("b"), Some(2048));
        assert_eq!(ini.get_size_mb("c"), Some(1024));
    }
}
