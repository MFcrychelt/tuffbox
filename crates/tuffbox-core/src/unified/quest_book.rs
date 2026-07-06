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
    let processed = snbt_preprocess(text);
    serde_json::from_str(&processed).map_err(|e| format!("SNBT parse: {}", e))
}

fn snbt_preprocess(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 5);
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => { out.push('"'); i += 1; while i < chars.len() && chars[i] != '"' { if chars[i] == '\\' { out.push('\\'); i += 1; if i < chars.len() { out.push(chars[i]); }} else { out.push(chars[i]); } i += 1; } if i < chars.len() { out.push('"'); i += 1; } }
            '[' => { if i + 3 < chars.len() && matches!(chars[i+1], 'I' | 'B' | 'L') { let mut j=i+2; while j<chars.len()&&chars[j].is_whitespace(){j+=1;} if j<chars.len()&&chars[j]==';'{out.push('[');i=j+1;continue;}} out.push('['); i+=1; }
            _ if c.is_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '.') { i += 1; }
                let ident: String = chars[start..i].iter().collect();
                let mut j = i;
                while j < chars.len() && chars[j].is_whitespace() { j += 1; }
                if j < chars.len() && chars[j] == ':' { out.push('"'); out.push_str(&ident); out.push('"'); out.extend(&chars[i..j+1]); i = j + 1; }
                else { out.push_str(&ident); }
            }
            _ if c.is_ascii_digit() || (c == '-' && i + 1 < chars.len() && chars[i+1].is_ascii_digit()) => {
                let start = i; i += 1;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') { i += 1; }
                if i < chars.len() && matches!(chars[i], 'b'|'s'|'L'|'f'|'d'|'B') && (i+1>=chars.len()||!chars[i+1].is_alphanumeric()) { out.extend(&chars[start..i]); i+=1; }
                else { out.extend(&chars[start..i]); }
            }
            _ => { out.push(c); i += 1; }
        }
    }
    out
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
