//! SNBT (Stringified NBT) parser for FTB Quests.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QuestBook {
    pub chapters: Vec<Chapter>,
    pub title: Option<String>,
    pub subtitle: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub id: String,
    pub title: String,
    pub icon: Option<String>,
    pub quests: Vec<Quest>,
    pub group: Option<String>,
    /// Relative path inside the project (e.g. config/ftbquests/quests/chapters/foo.snbt).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_file: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quest {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub description: Vec<String>,
    pub x: f64,
    pub y: f64,
    pub icon: Option<String>,
    pub dependencies: Vec<String>,
    pub tasks: Vec<Task>,
    pub rewards: Vec<Reward>,
    pub optional: bool,
    pub shape: Option<String>,
    pub size: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    #[serde(rename = "type")]
    pub task_type: String,
    pub title: Option<String>,
    pub value: Option<serde_json::Value>,
    pub properties: HashMap<String, serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reward {
    pub id: String,
    #[serde(rename = "type")]
    pub reward_type: String,
    pub title: Option<String>,
    pub properties: HashMap<String, serde_json::Value>,
}
#[derive(Debug, Clone)]
pub struct QuestValidationError {
    pub quest_id: String,
    pub message: String,
}

impl QuestBook {
    /// Resolve FTB Quests directory inside a project (config or defaultconfigs).
    pub fn quests_dir_for_project(project_dir: &std::path::Path) -> std::path::PathBuf {
        for rel in ["config/ftbquests/quests", "defaultconfigs/ftbquests/quests"] {
            let candidate = project_dir.join(rel);
            if candidate.is_dir() {
                return candidate;
            }
        }
        project_dir.join("config/ftbquests/quests")
    }

    pub fn load_from_project(project_dir: &std::path::Path) -> Result<Self, String> {
        let quests_dir = Self::quests_dir_for_project(project_dir);
        Self::load_from_dir(&quests_dir, project_dir)
    }

    pub fn load_from_dir(
        dir: &std::path::Path,
        project_dir: &std::path::Path,
    ) -> Result<Self, String> {
        let mut chapters = Vec::new();
        let chapter_dir = dir.join("chapters");
        let search_dir = if chapter_dir.is_dir() {
            chapter_dir
        } else {
            dir.to_path_buf()
        };
        if !search_dir.is_dir() {
            return Ok(Self::default());
        }
        for entry in std::fs::read_dir(&search_dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.extension().map_or(true, |e| e != "snbt") {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(mut ch) = parse_snbt_chapter(&content) {
                    ch.source_file = path
                        .strip_prefix(project_dir)
                        .ok()
                        .map(|p| p.to_string_lossy().replace('\\', "/"));
                    chapters.push(ch);
                }
            }
        }
        chapters.sort_by(|a, b| a.title.cmp(&b.title));
        Ok(QuestBook {
            chapters,
            ..Default::default()
        })
    }

    pub fn save_chapter(
        project_dir: &std::path::Path,
        chapter: &Chapter,
        relative_path: Option<&str>,
    ) -> Result<String, String> {
        let rel = relative_path
            .map(|s| s.to_string())
            .or_else(|| chapter.source_file.clone())
            .unwrap_or_else(|| {
                format!(
                    "config/ftbquests/quests/chapters/{}.snbt",
                    sanitize_snbt_filename(&chapter.id)
                )
            });
        let target = project_dir.join(&rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let snbt = serialize_chapter_to_snbt(chapter);
        std::fs::write(&target, snbt).map_err(|e| e.to_string())?;
        Ok(rel)
    }

    pub fn validate(&self) -> Vec<QuestValidationError> {
        let mut errors = Vec::new();
        let all_ids: std::collections::HashSet<String> = self
            .chapters
            .iter()
            .flat_map(|ch| ch.quests.iter().map(|q| q.id.clone()))
            .collect();
        for ch in &self.chapters {
            for q in &ch.quests {
                for dep in &q.dependencies {
                    if !all_ids.contains(dep) {
                        errors.push(QuestValidationError {
                            quest_id: q.id.clone(),
                            message: format!("Dep '{}' missing", dep),
                        });
                    }
                }
                if q.tasks.is_empty() {
                    errors.push(QuestValidationError {
                        quest_id: q.id.clone(),
                        message: "No tasks".into(),
                    });
                }
            }
        }
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
    let mut p = SnbtParser {
        chars: &chars,
        pos: 0,
    };
    p.skip_ws();
    let v = p.parse_value()?;
    p.skip_ws();
    if p.pos != p.chars.len() {
        return Err(format!(
            "SNBT parse: trailing content at position {}",
            p.pos
        ));
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
            Some(other) => Err(format!(
                "SNBT parse: unexpected char '{}' at {}",
                other, self.pos
            )),
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
                return Err(format!(
                    "SNBT parse: expected ':' after key at {}",
                    self.pos
                ));
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
        // SNBT numeric suffixes: 0.0d, 1L, 2f, etc.
        if matches!(
            self.peek(),
            Some('d' | 'D' | 'f' | 'F' | 'l' | 'L' | 'b' | 'B' | 's' | 'S')
        ) {
            self.pos += 1;
        }
        let s: String = self.chars[start..self.pos].iter().collect();
        let numeric = s.trim_end_matches(|c: char| {
            matches!(c, 'd' | 'D' | 'f' | 'F' | 'l' | 'L' | 'b' | 'B' | 's' | 'S')
        });
        if let Ok(i) = numeric.parse::<i64>() {
            return Ok(serde_json::Value::from(i));
        }
        if let Ok(f) = numeric.parse::<f64>() {
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
    let j = snbt_to_json(c)?;
    let m = j.as_object().ok_or("not object")?;
    Ok(Chapter {
        id: gs(m, "id").unwrap_or_else(|| "untitled".into()),
        title: gs(m, "title").unwrap_or_else(|| "Untitled".into()),
        icon: gs(m, "icon"),
        group: gs(m, "group"),
        quests: m
            .get("quests")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|q| parse_snbt_quest(q).ok()).collect())
            .unwrap_or_default(),
        source_file: None,
    })
}
fn parse_snbt_quest(v: &serde_json::Value) -> Result<Quest, String> {
    let m = v.as_object().ok_or("not object")?;
    let dependencies = m
        .get("dependencies")
        .map(parse_dependencies)
        .unwrap_or_default();
    Ok(Quest {
        id: gs(m, "id").unwrap_or_default(),
        title: gs(m, "title").unwrap_or_else(|| "Quest".into()),
        subtitle: gs(m, "subtitle"),
        description: m
            .get("description")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
        x: m.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
        y: m.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
        icon: gs(m, "icon"),
        dependencies,
        tasks: m
            .get("tasks")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|t| parse_snbt_task(t).ok()).collect())
            .unwrap_or_default(),
        rewards: m
            .get("rewards")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|r| parse_snbt_reward(r).ok()).collect())
            .unwrap_or_default(),
        optional: m.get("optional").and_then(|v| v.as_bool()).unwrap_or(false),
        shape: gs(m, "shape"),
        size: m.get("size").and_then(|v| v.as_f64()),
    })
}

fn parse_dependencies(v: &serde_json::Value) -> Vec<String> {
    if let Some(s) = v.as_str() {
        return s.split_whitespace().map(|x| x.to_string()).collect();
    }
    if let Some(arr) = v.as_array() {
        return arr
            .iter()
            .filter_map(|x| x.as_str().map(|s| s.to_string()))
            .collect();
    }
    Vec::new()
}
fn parse_snbt_task(v: &serde_json::Value) -> Result<Task, String> {
    let m = v.as_object().ok_or("not object")?;
    Ok(Task {
        id: gs(m, "id").unwrap_or_default(),
        task_type: gs(m, "type").unwrap_or_else(|| "item".into()),
        title: gs(m, "title"),
        value: m.get("value").cloned(),
        properties: m
            .iter()
            .filter(|(k, _)| !matches!(k.as_str(), "id" | "type" | "title" | "value"))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    })
}
fn parse_snbt_reward(v: &serde_json::Value) -> Result<Reward, String> {
    let m = v.as_object().ok_or("not object")?;
    Ok(Reward {
        id: gs(m, "id").unwrap_or_default(),
        reward_type: gs(m, "type").unwrap_or_else(|| "item".into()),
        title: gs(m, "title"),
        properties: m
            .iter()
            .filter(|(k, _)| !matches!(k.as_str(), "id" | "type" | "title"))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    })
}
fn gs(m: &serde_json::Map<String, serde_json::Value>, k: &str) -> Option<String> {
    m.get(k).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn sanitize_snbt_filename(id: &str) -> String {
    let cleaned: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "chapter".into()
    } else {
        cleaned
    }
}

fn snbt_quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

fn snbt_value(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => "null".into(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => snbt_quote(s),
        serde_json::Value::Array(items) => {
            let inner: Vec<String> = items.iter().map(snbt_value).collect();
            format!("[{}]", inner.join(" "))
        }
        serde_json::Value::Object(map) => {
            let inner: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("{}: {}", k, snbt_value(v)))
                .collect();
            format!("{{{}}}", inner.join(" "))
        }
    }
}

pub fn serialize_chapter_to_snbt(chapter: &Chapter) -> String {
    let mut lines = vec!["{".to_string()];
    lines.push(format!("\tid: {}", snbt_quote(&chapter.id)));
    lines.push(format!("\ttitle: {}", snbt_quote(&chapter.title)));
    if let Some(icon) = &chapter.icon {
        lines.push(format!("\ticon: {}", snbt_quote(icon)));
    }
    if let Some(group) = &chapter.group {
        lines.push(format!("\tgroup: {}", snbt_quote(group)));
    }
    lines.push("\tquests: [".to_string());
    for (qi, quest) in chapter.quests.iter().enumerate() {
        lines.push("\t\t{".to_string());
        lines.push(format!("\t\t\tid: {}", snbt_quote(&quest.id)));
        lines.push(format!("\t\t\ttitle: {}", snbt_quote(&quest.title)));
        if let Some(sub) = &quest.subtitle {
            lines.push(format!("\t\t\tsubtitle: {}", snbt_quote(sub)));
        }
        if !quest.description.is_empty() {
            let desc: Vec<String> = quest.description.iter().map(|d| snbt_quote(d)).collect();
            lines.push(format!("\t\t\tdescription: [{}]", desc.join(" ")));
        }
        lines.push(format!("\t\t\tx: {}d", quest.x));
        lines.push(format!("\t\t\ty: {}d", quest.y));
        if let Some(icon) = &quest.icon {
            lines.push(format!("\t\t\ticon: {}", snbt_quote(icon)));
        }
        if let Some(shape) = &quest.shape {
            lines.push(format!("\t\t\tshape: {}", snbt_quote(shape)));
        }
        if let Some(size) = quest.size {
            lines.push(format!("\t\t\tsize: {}d", size));
        }
        if quest.optional {
            lines.push("\t\t\toptional: true".to_string());
        }
        if !quest.dependencies.is_empty() {
            let deps: Vec<String> = quest.dependencies.iter().map(|d| snbt_quote(d)).collect();
            lines.push(format!("\t\t\tdependencies: [{}]", deps.join(" ")));
        }
        if !quest.tasks.is_empty() {
            lines.push("\t\t\ttasks: [".to_string());
            for task in &quest.tasks {
                lines.push("\t\t\t\t{".to_string());
                lines.push(format!("\t\t\t\t\tid: {}", snbt_quote(&task.id)));
                lines.push(format!("\t\t\t\t\ttype: {}", snbt_quote(&task.task_type)));
                if let Some(title) = &task.title {
                    lines.push(format!("\t\t\t\t\ttitle: {}", snbt_quote(title)));
                }
                if let Some(value) = &task.value {
                    lines.push(format!("\t\t\t\t\tvalue: {}", snbt_value(value)));
                }
                for (k, v) in &task.properties {
                    lines.push(format!("\t\t\t\t\t{}: {}", k, snbt_value(v)));
                }
                lines.push("\t\t\t\t}".to_string());
            }
            lines.push("\t\t\t]".to_string());
        }
        if !quest.rewards.is_empty() {
            lines.push("\t\t\trewards: [".to_string());
            for reward in &quest.rewards {
                lines.push("\t\t\t\t{".to_string());
                lines.push(format!("\t\t\t\t\tid: {}", snbt_quote(&reward.id)));
                lines.push(format!(
                    "\t\t\t\t\ttype: {}",
                    snbt_quote(&reward.reward_type)
                ));
                if let Some(title) = &reward.title {
                    lines.push(format!("\t\t\t\t\ttitle: {}", snbt_quote(title)));
                }
                for (k, v) in &reward.properties {
                    lines.push(format!("\t\t\t\t\t{}: {}", k, snbt_value(v)));
                }
                lines.push("\t\t\t\t}".to_string());
            }
            lines.push("\t\t\t]".to_string());
        }
        lines.push(if qi + 1 == chapter.quests.len() {
            "\t\t}".to_string()
        } else {
            "\t\t}".to_string()
        });
    }
    lines.push("\t]".to_string());
    lines.push("}".to_string());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrips_chapter() {
        let snbt = r#"{ title: "Test" id: "abc" quests: [{ id: "q1" title: "Q1" x: 0.0 y: 0.0 tasks: [{ id: "t1" type: "item" }] rewards: [{ id: "r1" type: "item" }] }] }"#;
        let ch = parse_snbt_chapter(snbt).unwrap();
        let out = serialize_chapter_to_snbt(&ch);
        let ch2 = parse_snbt_chapter(&out).unwrap();
        assert_eq!(ch.title, ch2.title);
        assert_eq!(ch.quests.len(), ch2.quests.len());
        assert_eq!(ch.quests[0].id, ch2.quests[0].id);
    }
}
