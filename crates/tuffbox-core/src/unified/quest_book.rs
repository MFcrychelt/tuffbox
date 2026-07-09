//! SNBT (Stringified NBT) parser for FTB Quests.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QuestBook { pub chapters: Vec<Chapter>, pub title: Option<String>, pub subtitle: Option<String> }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter { pub id: String, pub title: String, pub icon: Option<String>, pub quests: Vec<Quest>, pub group: Option<String> }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quest { pub id: String, pub title: String, pub subtitle: Option<String>, pub description: Vec<String>, pub x: f64, pub y: f64, pub icon: Option<String>, pub dependencies: Vec<String>, pub tasks: Vec<Task>, pub rewards: Vec<Reward>, pub optional: bool, pub shape: Option<String>, pub size: Option<f64> }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task { pub id: String, #[serde(rename = "type")] pub task_type: String, pub title: Option<String>, pub value: Option<serde_json::Value>, pub properties: HashMap<String, serde_json::Value> }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reward { pub id: String, #[serde(rename = "type")] pub reward_type: String, pub title: Option<String>, pub properties: HashMap<String, serde_json::Value> }
#[derive(Debug, Clone)]
pub struct QuestValidationError { pub quest_id: String, pub message: String }

impl QuestBook {
    pub fn load_from_dir(dir: &std::path::Path) -> Result<Self, String> {
        let mut chapters = Vec::new();
        let chapter_dir = dir.join("chapters");
        let search_dir = if chapter_dir.is_dir() { &chapter_dir } else { dir };
        if !search_dir.is_dir() { return Ok(Self::default()); }
        for entry in std::fs::read_dir(search_dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.extension().map_or(true, |e| e != "snbt") { continue; }
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(ch) = parse_snbt_chapter(&content) { chapters.push(ch); }
            }
        }
        Ok(QuestBook { chapters, ..Default::default() })
    }
    pub fn validate(&self) -> Vec<QuestValidationError> {
        let mut errors = Vec::new();
        let all_ids: std::collections::HashSet<String> = self.chapters.iter().flat_map(|ch| ch.quests.iter().map(|q| q.id.clone())).collect();
        for ch in &self.chapters { for q in &ch.quests {
            for dep in &q.dependencies { if !all_ids.contains(dep) { errors.push(QuestValidationError { quest_id: q.id.clone(), message: format!("Dep '{}' missing", dep) }); } }
            if q.tasks.is_empty() { errors.push(QuestValidationError { quest_id: q.id.clone(), message: "No tasks".into() }); }
        }}
        errors
    }
}

fn snbt_to_json(text: &str) -> Result<serde_json::Value, String> {
    parse_snbt(text)
}

/// Recursive-descent SNBT (Stringified NBT) parser.
/// SNBT uses the same structure as JSON but permits unquoted keys and
/// optional (whitespace or comma) separators between values.
fn parse_snbt(text: &str) -> Result<serde_json::Value, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut p = SnbtParser { chars: &chars, pos: 0 };
    p.skip_ws();
    let v = p.parse_value()?;
    p.skip_ws();
    if p.pos != p.chars.len() {
        return Err(format!("SNBT parse: trailing content at position {}", p.pos));
    }
    Ok(v)
}

struct SnbtParser<'a> {
    chars: &'a [char],
    pos: usize,
}

impl<'a> SnbtParser<'a> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }
    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }
    fn parse_value(&mut self) -> Result<serde_json::Value, String> {
        self.skip_ws();
        match self.peek() {
            Some('{') => self.parse_object(),
            Some('[') => self.parse_array(),
            Some('"') => Ok(serde_json::Value::String(self.parse_string()?)),
            Some(c) if c.is_ascii_digit() || c == '-' => self.parse_number(),
            Some(c) if c.is_alphabetic() || c == '_' => self.parse_ident_value(),
            Some(other) => Err(format!("SNBT parse: unexpected char '{}' at {}", other, self.pos)),
            None => Err("SNBT parse: unexpected end of input".into()),
        }
    }
    fn parse_object(&mut self) -> Result<serde_json::Value, String> {
        self.pos += 1; // consume '{'
        let mut map = serde_json::Map::new();
        self.skip_ws();
        if self.peek() == Some('}') {
            self.pos += 1;
            return Ok(serde_json::Value::Object(map));
        }
        loop {
            self.skip_ws();
            let key = self.parse_key()?;
            self.skip_ws();
            if self.peek() != Some(':') {
                return Err(format!("SNBT parse: expected ':' after key at {}", self.pos));
            }
            self.pos += 1; // consume ':'
            let val = self.parse_value()?;
            map.insert(key, val);
            self.skip_ws();
            match self.peek() {
                Some(',') => {
                    self.pos += 1;
                    continue;
                }
                Some('}') => {
                    self.pos += 1;
                    break;
                }
                Some(_) => continue, // SNBT allows whitespace-only separators
                None => return Err("SNBT parse: unexpected end of input in object".into()),
            }
        }
        Ok(serde_json::Value::Object(map))
    }
    fn parse_array(&mut self) -> Result<serde_json::Value, String> {
        self.pos += 1; // consume '['
        let mut arr = Vec::new();
        self.skip_ws();
        if self.peek() == Some(']') {
            self.pos += 1;
            return Ok(serde_json::Value::Array(arr));
        }
        loop {
            let v = self.parse_value()?;
            arr.push(v);
            self.skip_ws();
            match self.peek() {
                Some(',') => {
                    self.pos += 1;
                    continue;
                }
                Some(']') => {
                    self.pos += 1;
                    break;
                }
                Some(_) => continue,
                None => return Err("SNBT parse: unexpected end of input in array".into()),
            }
        }
        Ok(serde_json::Value::Array(arr))
    }
    fn parse_string(&mut self) -> Result<String, String> {
        self.pos += 1; // consume opening quote
        let mut s = String::new();
        while let Some(c) = self.peek() {
            self.pos += 1;
            match c {
                '"' => return Ok(s),
                '\\' => {
                    if let Some(e) = self.peek() {
                        self.pos += 1;
                        s.push(match e {
                            'n' => '\n',
                            't' => '\t',
                            'r' => '\r',
                            '\\' => '\\',
                            '"' => '"',
                            '\'' => '\'',
                            '/' => '/',
                            'b' => '\u{08}',
                            'f' => '\u{0C}',
                            other => other,
                        });
                    }
                }
                _ => s.push(c),
            }
        }
        Err("SNBT parse: unterminated string".into())
    }
    fn parse_key(&mut self) -> Result<String, String> {
        match self.peek() {
            Some('"') => self.parse_string(),
            Some(c) if c.is_alphabetic() || c == '_' => {
                let start = self.pos;
                while let Some(ch) = self.peek() {
                    if ch.is_alphanumeric() || ch == '_' || ch == '.' {
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
                Ok(self.chars[start..self.pos].iter().collect())
            }
            _ => Err(format!("SNBT parse: expected key at {}", self.pos)),
        }
    }
    fn parse_number(&mut self) -> Result<serde_json::Value, String> {
        let start = self.pos;
        if self.peek() == Some('-') {
            self.pos += 1;
        }
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E' || c == '-' || c == '+' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let s: String = self.chars[start..self.pos].iter().collect();
        if let Ok(i) = s.parse::<i64>() {
            return Ok(serde_json::Value::from(i));
        }
        if let Ok(f) = s.parse::<f64>() {
            return Ok(serde_json::Value::from(f));
        }
        Err(format!("SNBT parse: invalid number '{}'", s))
    }
    fn parse_ident_value(&mut self) -> Result<serde_json::Value, String> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' || c == '.' || c == '-' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let s: String = self.chars[start..self.pos].iter().collect();
        match s.as_str() {
            "true" => Ok(serde_json::Value::Bool(true)),
            "false" => Ok(serde_json::Value::Bool(false)),
            "null" => Ok(serde_json::Value::Null),
            _ => Ok(serde_json::Value::String(s)),
        }
    }
}

fn parse_snbt_chapter(c: &str) -> Result<Chapter, String> {
    let j = snbt_to_json(c)?; let m = j.as_object().ok_or("not object")?;
    Ok(Chapter { id: gs(m,"id").unwrap_or_else(|| "untitled".into()), title: gs(m,"title").unwrap_or_else(|| "Untitled".into()), icon: gs(m,"icon"), group: gs(m,"group"), quests: m.get("quests").and_then(|v|v.as_array()).map(|a|a.iter().filter_map(|q|parse_snbt_quest(q).ok()).collect()).unwrap_or_default() })
}
fn parse_snbt_quest(v: &serde_json::Value) -> Result<Quest, String> {
    let m = v.as_object().ok_or("not object")?;
    Ok(Quest { id: gs(m,"id").unwrap_or_default(), title: gs(m,"title").unwrap_or_else(|| "Quest".into()), subtitle: gs(m,"subtitle"), description: m.get("description").and_then(|v|v.as_array()).map(|a|a.iter().filter_map(|x|x.as_str().map(|s|s.to_string())).collect()).unwrap_or_default(), x: m.get("x").and_then(|v|v.as_f64()).unwrap_or(0.0), y: m.get("y").and_then(|v|v.as_f64()).unwrap_or(0.0), icon: gs(m,"icon"), dependencies: gs(m,"dependencies").map(|s|s.split_whitespace().map(|x|x.to_string()).collect()).unwrap_or_default(), tasks: m.get("tasks").and_then(|v|v.as_array()).map(|a|a.iter().filter_map(|t|parse_snbt_task(t).ok()).collect()).unwrap_or_default(), rewards: m.get("rewards").and_then(|v|v.as_array()).map(|a|a.iter().filter_map(|r|parse_snbt_reward(r).ok()).collect()).unwrap_or_default(), optional: m.get("optional").and_then(|v|v.as_bool()).unwrap_or(false), shape: gs(m,"shape"), size: m.get("size").and_then(|v|v.as_f64()) })
}
fn parse_snbt_task(v: &serde_json::Value) -> Result<Task, String> {
    let m = v.as_object().ok_or("not object")?;
    Ok(Task { id: gs(m,"id").unwrap_or_default(), task_type: gs(m,"type").unwrap_or_else(|| "item".into()), title: gs(m,"title"), value: m.get("value").cloned(), properties: m.iter().filter(|(k,_)|!matches!(k.as_str(),"id"|"type"|"title"|"value")).map(|(k,v)|(k.clone(),v.clone())).collect() })
}
fn parse_snbt_reward(v: &serde_json::Value) -> Result<Reward, String> {
    let m = v.as_object().ok_or("not object")?;
    Ok(Reward { id: gs(m,"id").unwrap_or_default(), reward_type: gs(m,"type").unwrap_or_else(|| "item".into()), title: gs(m,"title"), properties: m.iter().filter(|(k,_)|!matches!(k.as_str(),"id"|"type"|"title")).map(|(k,v)|(k.clone(),v.clone())).collect() })
}
fn gs(m: &serde_json::Map<String, serde_json::Value>, k: &str) -> Option<String> { m.get(k).and_then(|v|v.as_str()).map(|s|s.to_string()) }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_simple() {
        let snbt = r#"{ title: "Test" id: "abc" quests: [{ id: "q1" title: "Q1" x: 0.0 y: 0.0 tasks: [{ id: "t1" type: "item" }] rewards: [{ id: "r1" type: "item" }] }] }"#;
        let ch = parse_snbt_chapter(snbt).unwrap();
        assert_eq!(ch.title, "Test");
        assert_eq!(ch.quests.len(), 1);
    }
}
