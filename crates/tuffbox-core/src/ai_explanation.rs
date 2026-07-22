//! AI crash explanation infrastructure.
//!
//! Builds structured context from crash data for LLM consumption.
//! This module does NOT call any LLM API directly — it prepares the
//! context, prompt templates, and parsing logic that the Tauri
//! backend can use with any LLM provider (OpenAI, Anthropic, local).

use crate::crash_assistant::CrashAnalysisFinding;
use crate::crash_kb::{SimilarCaseHit, smart_excerpt};
use crate::project_ai_inventory::ProjectAiInventory;
use serde::{Deserialize, Serialize};

/// Context passed to an AI model for crash explanation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashAiContext {
    pub mc_version: String,
    pub loader: String,
    pub loader_version: String,
    pub java_version: String,
    pub os: String,
    /// Truncated list (suspects / sample) — prefer inventory for full list.
    pub installed_mods: Vec<String>,
    #[serde(default)]
    pub installed_mod_count: u32,
    pub crash_report_excerpt: String,
    pub latest_log_excerpt: String,
    pub suspected_mods: Vec<String>,
    pub crash_assistant_findings: Vec<CrashAiFinding>,
    pub recent_changes: Vec<String>,
    pub graph_diagnostics: Vec<String>,
    #[serde(default)]
    pub similar_cases: Vec<SimilarCaseHit>,
    #[serde(default)]
    pub fingerprint_key: String,
    #[serde(default)]
    pub report_id: Option<String>,
    /// Full project inventory (mods, packs, datapacks, configs).
    #[serde(default)]
    pub inventory: Option<ProjectAiInventory>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashAiFinding {
    pub code: String,
    pub title: String,
    pub description: String,
    pub auto_fix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashAiResponse {
    pub human_explanation: String,
    pub confidence: f64,
    pub suspected_mods: Vec<String>,
    pub recommended_actions: Vec<AiAction>,
    pub needs_user_review: bool,
    pub additional_context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAction {
    pub action_type: String, // "update", "remove", "install", "disable", "config_change"
    pub mod_id: Option<String>,
    pub description: String,
    pub risk: String, // "low", "medium", "high"
}

/// Prefer [`crate::action_plan::ACTION_PLAN_JSON_SCHEMA_HINT`] for executable plans.
/// Kept as an alias so existing call sites keep compiling.
pub const CRASH_JSON_SCHEMA_HINT: &str = crate::action_plan::ACTION_PLAN_JSON_SCHEMA_HINT;

/// Builds a structured prompt for the AI to explain a crash.
pub fn build_crash_prompt(ctx: &CrashAiContext) -> String {
    let mut p = String::new();

    p.push_str(crate::action_plan::ACTION_PLAN_SYSTEM_PROMPT);
    p.push_str("\n\n## System Context\n");
    p.push_str(&format!("- Minecraft: {}\n", ctx.mc_version));
    p.push_str(&format!("- Loader: {} {}\n", ctx.loader, ctx.loader_version));
    p.push_str(&format!("- Java: {}\n", ctx.java_version));
    p.push_str(&format!("- OS: {}\n", ctx.os));
    p.push_str(&format!(
        "- Installed mods: {} (ids listed only when relevant)\n\n",
        if ctx.installed_mod_count > 0 {
            ctx.installed_mod_count
        } else {
            ctx.installed_mods.len() as u32
        }
    ));
    if let Some(ref id) = ctx.report_id {
        p.push_str(&format!("- Crash report id: {id}\n"));
    }
    if !ctx.fingerprint_key.is_empty() {
        p.push_str(&format!("- Fingerprint: {}\n\n", ctx.fingerprint_key));
    }

    if !ctx.suspected_mods.is_empty() {
        p.push_str("## Suspected Mods\n");
        for m in &ctx.suspected_mods {
            p.push_str(&format!("- {m}\n"));
        }
        p.push('\n');
    }

    if !ctx.crash_assistant_findings.is_empty() {
        p.push_str("## Automated Analysis Results\n");
        for f in &ctx.crash_assistant_findings {
            p.push_str(&format!("- [{}] {}: {}\n", f.code, f.title, f.description));
            if let Some(fix) = &f.auto_fix {
                p.push_str(&format!("  Auto-fix: {fix}\n"));
            }
        }
        p.push('\n');
    }

    if !ctx.similar_cases.is_empty() {
        p.push_str("## Similar known cases (from local knowledge base)\n");
        p.push_str("Prefer these solutions when they match. Do not invent mods outside the project inventory.\n");
        for (i, c) in ctx.similar_cases.iter().enumerate() {
            p.push_str(&format!(
                "{}. score={:.2} source={} key={}\n   Solution: {}\n",
                i + 1,
                c.score,
                c.source,
                c.fingerprint_key,
                c.solution
            ));
            if !c.actions.is_empty() {
                for a in &c.actions {
                    p.push_str(&format!(
                        "   Action: {} {} — {} (risk {})\n",
                        a.action_type,
                        a.mod_id.as_deref().unwrap_or("-"),
                        a.description,
                        a.risk
                    ));
                }
            }
        }
        p.push('\n');
    }

    if let Some(ref inv) = ctx.inventory {
        p.push_str(&crate::project_ai_inventory::format_inventory_for_prompt(
            inv, 14000,
        ));
        p.push('\n');
    }

    if !ctx.graph_diagnostics.is_empty() {
        p.push_str("## Graph Diagnostics\n");
        for d in &ctx.graph_diagnostics {
            p.push_str(&format!("- {d}\n"));
        }
        p.push('\n');
    }

    if !ctx.recent_changes.is_empty() {
        p.push_str("## Recent Changes (may have caused the crash)\n");
        for c in &ctx.recent_changes {
            p.push_str(&format!("- {c}\n"));
        }
        p.push('\n');
    }

    p.push_str("## Crash Report (excerpt)\n```\n");
    p.push_str(&truncate(&smart_excerpt(&ctx.crash_report_excerpt, 4500), 4500));
    p.push_str("\n```\n\n");

    p.push_str("## Latest Log (excerpt)\n```\n");
    p.push_str(&truncate(&smart_excerpt(&ctx.latest_log_excerpt, 3200), 3200));
    p.push_str("\n```\n\n");

    p.push_str("## Instructions\n");
    p.push_str(CRASH_JSON_SCHEMA_HINT);
    p.push_str("\n\nFollow the system rules above. Prefer `actions` with `op` fields over legacy recommended_actions.\n");

    p
}

/// Builds a shorter prompt for quick crash triage.
pub fn build_triage_prompt(ctx: &CrashAiContext) -> String {
    format!(
        "Minecraft {} crashed on {} {} with Java {}. {} mods installed. Crash excerpt: {}",
        ctx.mc_version,
        ctx.loader,
        ctx.loader_version,
        ctx.java_version,
        if ctx.installed_mod_count > 0 {
            ctx.installed_mod_count
        } else {
            ctx.installed_mods.len() as u32
        },
        truncate(&ctx.crash_report_excerpt, 1000),
    )
}

/// Builds a prompt for mod compatibility analysis.
pub fn build_compat_prompt(mods_to_check: &[(String, String)]) -> String {
    let mut p = String::from(
        "Analyze these Minecraft mod combinations for known compatibility issues:\n\n",
    );
    for (a, b) in mods_to_check {
        p.push_str(&format!("- {a} + {b}\n"));
    }
    p.push_str("\nRespond with JSON: [{mod_a, mod_b, compatible, reason}]\n");
    p
}

/// Parses an AI response JSON into structured data.
/// Accepts both ActionPlan (`actions`/`op`) and legacy `recommended_actions`.
pub fn parse_crash_response(json_str: &str) -> Result<CrashAiResponse, String> {
    let plan = crate::action_plan::parse_action_plan(json_str)?;
    Ok(CrashAiResponse {
        human_explanation: plan.human_explanation.clone(),
        confidence: plan.confidence,
        suspected_mods: plan.suspected_mods.clone(),
        recommended_actions: crate::action_plan::plan_to_legacy_ai_actions(&plan),
        needs_user_review: plan.needs_user_review,
        additional_context: plan.additional_context,
    })
}

/// Converts CrashAssistant findings to AI-compatible format.
pub fn findings_to_ai(findings: &[CrashAnalysisFinding]) -> Vec<CrashAiFinding> {
    findings
        .iter()
        .map(|f| CrashAiFinding {
            code: f.code.clone(),
            title: f.title.clone(),
            description: f.description.clone(),
            auto_fix: f.auto_fix.clone(),
        })
        .collect()
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }
    let cut = &s[..max_len];
    format!("{cut}... (truncated)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_prompt() {
        let ctx = CrashAiContext {
            mc_version: "1.20.1".into(),
            loader: "fabric".into(),
            loader_version: "0.15".into(),
            java_version: "17".into(),
            os: "Windows 11".into(),
            installed_mods: vec!["sodium".into()],
            installed_mod_count: 2,
            crash_report_excerpt: "NoClassDefFoundError: com/example/Foo".into(),
            latest_log_excerpt: "Mixin apply failed".into(),
            suspected_mods: vec!["sodium".into()],
            crash_assistant_findings: vec![],
            recent_changes: vec!["Added iris 1.7.0".into()],
            graph_diagnostics: vec!["Missing dependency: indium".into()],
            similar_cases: vec![],
            fingerprint_key: "test".into(),
            report_id: Some("crash-2024".into()),
            inventory: None,
        };
        let prompt = build_crash_prompt(&ctx);
        assert!(prompt.contains("iris"));
        assert!(prompt.contains("Mixin"));
        assert!(prompt.contains("humanExplanation") || prompt.contains("schemaVersion"));
    }

    #[test]
    fn parses_response() {
        let json = r#"{"human_explanation":"Test","confidence":0.8,"suspected_mods":["sodium"],"recommended_actions":[{"action_type":"install","mod_id":"indium","description":"Install Indium","risk":"low"}],"needs_user_review":true}"#;
        let r = parse_crash_response(json).unwrap();
        assert_eq!(r.human_explanation, "Test");
        assert_eq!(r.suspected_mods, vec!["sodium"]);
    }
}
