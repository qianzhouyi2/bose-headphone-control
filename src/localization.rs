use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pack {
    pub code: String,
    pub native_name: String,
    pub strings: BTreeMap<String, String>,
}
pub struct Localization {
    pub preference: String,
    pub packs: BTreeMap<String, Pack>,
    pub settings: Option<PathBuf>,
    pub system: String,
}
pub fn data_dir() -> PathBuf {
    PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default()).join("Quiet Rebellion")
}
impl Localization {
    pub fn new() -> Self {
        let mut packs = BTreeMap::new();
        for contents in [
            include_str!("../languages/en.json"),
            include_str!("../languages/zh-CN.json"),
        ] {
            let p: Pack = serde_json::from_str(contents).expect("embedded language catalog");
            packs.insert(p.code.clone(), p);
        }
        let mut locale = [0u16; 85];
        unsafe {
            let language = windows::Win32::Globalization::GetUserDefaultUILanguage();
            windows::Win32::Globalization::LCIDToLocaleName(language as u32, Some(&mut locale), 0);
        }
        let system =
            String::from_utf16_lossy(&locale[..locale.iter().position(|c| *c == 0).unwrap_or(0)]);
        let settings = Some(data_dir().join("settings.json"));
        let mut result = Self {
            preference: "system".into(),
            packs,
            settings,
            system,
        };
        if let Ok(exe) = std::env::current_exe() {
            if let Ok(files) =
                std::fs::read_dir(exe.parent().unwrap_or(Path::new(".")).join("Languages"))
            {
                for entry in files.flatten() {
                    if entry.path().extension().is_some_and(|x| x == "json") {
                        if let Ok(text) = std::fs::read_to_string(entry.path()) {
                            if let Ok(pack) = serde_json::from_str(&text) {
                                let _ = result.register(pack);
                            }
                        }
                    }
                }
            }
        }
        if let Some(path) = &result.settings {
            if let Ok(text) = std::fs::read_to_string(path) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(language) = v["language"].as_str() {
                        if language == "system" || result.packs.contains_key(language) {
                            result.preference = language.into();
                        }
                    }
                }
            }
        }
        result
    }
    pub fn code(&self) -> &str {
        let code = if self.preference == "system" {
            &self.system
        } else {
            &self.preference
        };
        if self.packs.contains_key(code) {
            code
        } else if code == "zh" || code.starts_with("zh-Hans") || code == "zh-SG" {
            "zh-CN"
        } else {
            let base = code.split('-').next().unwrap_or("en");
            if self.packs.contains_key(base) {
                base
            } else {
                "en"
            }
        }
    }
    pub fn text(&self, key: &str) -> String {
        self.packs[self.code()]
            .strings
            .get(key)
            .or_else(|| self.packs["en"].strings.get(key))
            .cloned()
            .unwrap_or_else(|| key.into())
    }
    pub fn register(&mut self, pack: Pack) -> Result<()> {
        ensure!(
            !self.packs.contains_key(&pack.code)
                && pack.code.len() <= 32
                && !pack.native_name.trim().is_empty()
                && pack
                    .code
                    .split('-')
                    .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric())),
            "Invalid language pack"
        );
        for (key, text) in &pack.strings {
            if let Some(base) = self.packs["en"].strings.get(key) {
                ensure!(slots(text) == slots(base), "Invalid placeholders for {key}");
            }
        }
        self.packs.insert(pack.code.clone(), pack);
        Ok(())
    }
    pub fn set(&mut self, code: String) -> Result<()> {
        ensure!(
            code == "system" || self.packs.contains_key(&code),
            "Unknown language"
        );
        if let Some(path) = &self.settings {
            std::fs::create_dir_all(path.parent().unwrap())?;
            let mut value = std::fs::read_to_string(path)
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .filter(|v| v.is_object())
                .unwrap_or_else(|| serde_json::json!({}));
            value["language"] = code.clone().into();
            let temp = path.with_extension("json.tmp");
            std::fs::write(&temp, serde_json::to_vec_pretty(&value)?)?;
            std::fs::rename(temp, path)?;
        }
        self.preference = code;
        Ok(())
    }
}
fn slots(text: &str) -> Vec<String> {
    let mut result = vec![];
    let mut iter = text.chars().peekable();
    while let Some(c) = iter.next() {
        if c == '{' {
            if iter.peek() == Some(&'{') {
                iter.next();
                continue;
            }
            let s: String = iter.by_ref().take_while(|c| *c != '}').collect();
            result.push(s);
        }
    }
    result.sort();
    result
}
