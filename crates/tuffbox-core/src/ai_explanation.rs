//! AI crash explanation infrastructure.
//!
//! Builds structured context from crash data for LLM consumption.
//! This module does NOT call any LLM API directly — it prepares the
//! context, prompt templates, and parsing logic that the Tauri
//! backend can use with any LLM provider (OpenAI, Anthropic, local).

use crate::crash_assistant::CrashAnalysisFinding;
use serde::{Deserialize, Serialize};

/// Context passed to an AI model for crash explanation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashAiContext {
    pub mc_version: String,
    pub loader: String,
    pub loader_version: String,
    pub java_version: String,
    pub os: String,
    pub installed_mods: Vec<String>,
    pub crash_report_excerpt: String,
    pub latest_log_excerpt: String,
    pub suspected_mods: Vec<String>,
    pub crash_assistant_findings: Vec<CrashAiFinding>,
    pub recent_changes: Vec<String>,
    pub graph_diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashAiFinding {
    pub code: String,
    pub title: String,
    pub description: String,
    pub auto_fix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashAiResponse {
    pub human_explanation: String,
    pub confidence: f64,
    pub suspected_mods: Vec<String>,
    pub recommended_actions: Vec<AiAction>,
    pub needs_user_review: bool,
    pub additional_context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiAction {
    pub action_type: String,  // "update", "remove", "install", "disable", "config_change"
    pub mod_id: Option<String>,
    pub description: String,
    pub risk: String,  // "low", "medium", "high"
}

/// Builds a structured prompt for the AI to explain a crash.
pub fn build_crash_prompt(ctx: &CrashAiContext) -> String {
    let mut p = String::new();

    p.push_str("You are TuffBox AI, a Minecraft modpack crash analyzer.

");
    p.push_str("## System Context
");
    p.push_str(&format!("- Minecraft: {}
", ctx.mc_version));
    p.push_str(&format!("- Loader: {} {}
", ctx.loader, ctx.loader_version));
    p.push_str(&format!("- Java: {}
", ctx.java_version));
    p.push_str(&format!("- OS: {}
", ctx.os));
    p.push_str(&format!("- Installed mods: {}

", ctx.installed_mods.len()));

    if !ctx.suspected_mods.is_empty() {
        p.push_str("## Suspected Mods
");
        for m in &ctx.suspected_mods { p.push_str(&format!("- {}
", m)); }
        p.push('\n');
    }

    if !ctx.crash_assistant_findings.is_empty() {
        p.push_str("## Automated Analysis Results\n");
        for f in &ctx.crash_assistant_findings {
            p.push_str(&format!("- [{}] {}: {}\n", f.code, f.title, f.description));
            if let Some(fix) = &f.auto_fix { p.push_str(&format!("  Auto-fix: {}\n", fix)); }
        }
        p.push('\n');
    }

    if !ctx.graph_diagnostics.is_empty() {
        p.push_str("## Graph Diagnostics\n");
        for d in &ctx.graph_diagnostics { p.push_str(&format!("- {}\n", d)); }
        p.push('\n');
    }

    if !ctx.recent_changes.is_empty() {
        p.push_str("## Recent Changes (may have caused the crash)\n");
        for c in &ctx.recent_changes { p.push_str(&format!("- {}\n", c)); }
        p.push('\n');
    }

    p.push_str("## Crash Report (excerpt)
```
");
    p.push_str(&truncate(&ctx.crash_report_excerpt, 4000));
    p.push_str("
```

");

    p.push_str("## Latest Log (excerpt)
```
");
    p.push_str(&truncate(&ctx.latest_log_excerpt, 3000));
    p.push_str("
```

");

    p.push_str("## Instructions
");
    p.push_str("Analyze the crash and respond with a JSON object containing:
");
    p.push_str("- human_explanation: A clear explanation in plain English
");
    p.push_str("- confidence: 0.0-1.0 how confident you are
");
    p.push_str("- suspected_mods: list of mod IDs most likely responsible
");
    p.push_str("- recommended_actions: array of {action_type, mod_id?, description, risk}
");
    p.push_str("- needs_user_review: true if user should verify before applying
");
    p.push_str("- additional_context: any extra helpful information
");

    p
}

/// Builds a shorter prompt for quick crash triage.
pub fn build_triage_prompt(ctx: &CrashAiContext) -> String {
    format!(
        "Minecraft {} crashed on {} {} with Java {}. {} mods installed. Crash excerpt: {}",
        ctx.mc_version, ctx.loader, ctx.loader_version, ctx.java_version,
        ctx.installed_mods.len(),
        truncate(&ctx.crash_report_excerpt, 1000),
    )
}

/// Builds a prompt for mod compatibility analysis.
pub fn build_compat_prompt(mods_to_check: &[(String, String)]) -> String {
    let mut p = String::from(
        "Analyze these Minecraft mod combinations for known compatibility issues:

"
    );
    for (a, b) in mods_to_check {
        p.push_str(&format!("- {} + {}
", a, b));
    }
    p.push_str("
Respond with JSON: [{mod_a, mod_b, compatible, reason}]
");
    p
}

/// Parses an AI response JSON into structured data.
pub fn parse_crash_response(json_str: &str) -> Result<CrashAiResponse, String> {
    let v: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| format!("Invalid AI response JSON: {}", e))?;

    Ok(CrashAiResponse {
        human_explanation: v.get("human_explanation").and_then(|s| s.as_str()).unwrap_or("No explanation provided.").into(),
        confidence: v.get("confidence").and_then(|c| c.as_f64()).unwrap_or(0.5),
        suspected_mods: v.get("suspected_mods").and_then(|s| s.as_array()).map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()).unwrap_or_default(),
        recommended_actions: v.get("recommended_actions").and_then(|a| a.as_array()).map(|a| a.iter().filter_map(|act| Some(AiAction {
            action_type: act.get("action_type").and_then(|s| s.as_str()).unwrap_or("unknown").into(),
            mod_id: act.get("mod_id").and_then(|s| s.as_str()).map(|s| s.into()),
            description: act.get("description").and_then(|s| s.as_str()).unwrap_or("").into(),
            risk: act.get("risk").and_then(|s| s.as_str()).unwrap_or("medium").into(),
        })).collect()).unwrap_or_default(),
        needs_user_review: v.get("needs_user_review").and_then(|b| b.as_bool()).unwrap_or(true),
        additional_context: v.get("additional_context").and_then(|s| s.as_str()).map(|s| s.into()),
    })
}

/// Converts CrashAssistant findings to AI-compatible format.
pub fn findings_to_ai(findings: &[CrashAnalysisFinding]) -> Vec<CrashAiFinding> {
    findings.iter().map(|f| CrashAiFinding {
        code: f.code.clone(),
        title: f.title.clone(),
        description: f.description.clone(),
        auto_fix: f.auto_fix.clone(),
    }).collect()
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len { return s.to_string(); }
    let cut = &s[..max_len];
    format!("{}... (truncated)", cut)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_prompt() {
        let ctx = CrashAiContext {
            mc_version: "1.20.1".into(), loader: "fabric".into(), loader_version: "0.15".into(),
            java_version: "17".into(), os: "Windows 11".into(),
            installed_mods: vec!["sodium".into(), "iris".into()],
            crash_report_excerpt: "NoClassDefFoundError: com/example/Foo".into(),
            latest_log_excerpt: "Mixin apply failed".into(),
            suspected_mods: vec!["sodium".into()],
            crash_assistant_findings: vec![],
            recent_changes: vec!["Added iris 1.7.0".into()],
            graph_diagnostics: vec!["Missing dependency: indium".into()],
        };
        let prompt = build_crash_prompt(&ctx);
        assert!(prompt.contains("iris"));
        assert!(prompt.contains("Mixin"));
    }

    #[test]
    fn parses_response() {
        let json = r#"{"human_explanation":"Test","confidence":0.8,"suspected_mods":["sodium"],"recommended_actions":[{"action_type":"install","mod_id":"indium","description":"Install Indium","risk":"low"}],"needs_user_review":true}"#;
        let r = parse_crash_response(json).unwrap();
        assert_eq!(r.human_explanation, "Test");
        assert_eq!(r.suspected_mods, vec!["sodium"]);
    }
}
