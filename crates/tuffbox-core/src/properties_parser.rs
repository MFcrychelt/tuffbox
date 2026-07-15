//! Server/options.properties parser for Minecraft config files.
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct PropertiesFile {
    pub entries: Vec<PropertyEntry>,
}

#[derive(Debug, Clone)]
pub struct PropertyEntry {
    pub key: String,
    pub value: String,
    pub comment_before: Option<String>,
}

impl PropertiesFile {
    pub fn parse(content: &str) -> Self {
        let mut entries = Vec::new();
        let mut pending_comment = String::new();
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                pending_comment.clear();
                continue;
            }
            if trimmed.starts_with('#') || trimmed.starts_with('!') {
                if !pending_comment.is_empty() {
                    pending_comment.push('\n');
                }
                pending_comment.push_str(trimmed);
                continue;
            }
            if let Some(eq_pos) = trimmed.find('=') {
                let key = trimmed[..eq_pos].trim().to_string();
                let value = trimmed[eq_pos + 1..].trim().to_string();
                entries.push(PropertyEntry {
                    key,
                    value,
                    comment_before: if pending_comment.is_empty() {
                        None
                    } else {
                        Some(pending_comment.clone())
                    },
                });
                pending_comment.clear();
            }
        }
        Self { entries }
    }

    pub fn to_map(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for e in &self.entries {
            map.insert(e.key.clone(), e.value.clone());
        }
        map
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.key == key)
            .map(|e| e.value.as_str())
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.get(key)
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
    }

    pub fn get_int(&self, key: &str) -> Option<i64> {
        self.get(key).and_then(|v| v.parse().ok())
    }

    pub fn set(&mut self, key: &str, value: &str) {
        for e in &mut self.entries {
            if e.key == key {
                e.value = value.to_string();
                return;
            }
        }
        self.entries.push(PropertyEntry {
            key: key.into(),
            value: value.into(),
            comment_before: None,
        });
    }

    pub fn to_string(&self) -> String {
        let mut out = String::new();
        for e in &self.entries {
            if let Some(c) = &e.comment_before {
                out.push_str(c);
                out.push('\n');
            }
            out.push_str(&format!("{}={}\n", e.key, e.value));
        }
        out
    }

    pub fn minecraft_defaults() -> HashMap<&'static str, &'static str> {
        HashMap::from([
            ("server-port", "25565"),
            ("max-players", "20"),
            ("view-distance", "10"),
            ("simulation-distance", "10"),
            ("online-mode", "true"),
            ("difficulty", "normal"),
            ("gamemode", "survival"),
            ("enable-command-block", "false"),
            ("spawn-protection", "16"),
            ("max-tick-time", "60000"),
            ("level-name", "world"),
            ("allow-flight", "false"),
            ("pvp", "true"),
            ("spawn-npcs", "true"),
            ("spawn-animals", "true"),
            ("spawn-monsters", "true"),
            ("generate-structures", "true"),
            ("allow-nether", "true"),
            ("force-gamemode", "false"),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses() {
        let p = PropertiesFile::parse("server-port=25565\nmax-players=20\n");
        assert_eq!(p.get("server-port"), Some("25565"));
    }
    #[test]
    fn bools() {
        let p = PropertiesFile::parse("a=true\nb=false\n");
        assert_eq!(p.get_bool("a"), Some(true));
    }
    #[test]
    fn roundtrip() {
        let t = "server-port=25565\nmax-players=20\n";
        assert_eq!(PropertiesFile::parse(t).to_string(), t);
    }
    #[test]
    fn set_val() {
        let mut p = PropertiesFile::parse("a=1\n");
        p.set("a", "2");
        p.set("b", "3");
        assert_eq!(p.get("a"), Some("2"));
    }
}
