//! Executable ActionPlan protocol for crash diagnosis (server + local AI).
//!
//! The launcher validates this JSON and applies ops via FixAction / ChangeAction.
//! AI never mutates files directly.

use crate::ai_explanation::AiAction;
use crate::change_plan::{ChangeAction, ChangePlan, ChangeRisk};
use crate::crash::FixAction;
use crate::graph::NodeId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const ACTION_PLAN_SCHEMA_VERSION: u32 = 1;

/// Canonical system prompt shared by server and local LLM paths.
pub const ACTION_PLAN_SYSTEM_PROMPT: &str = r#"You are TuffBox Crash Planner. You only output ONE JSON object matching schemaVersion 1.
You do NOT apply fixes. You propose an ActionPlan for the launcher.

Rules:
1. Prefer matchedCaseIds / similar known cases when score is high.
2. Every mutating action MUST include modId (except pure edit_config).
3. Only reference mods that appear in inventory OR are explicit missing dependencies named in the crash.
4. Prefer disable_mod before remove_mod; prefer exact version when known.
5. For edit_config: path relative to instance; patch must be minimal; never rewrite whole unrelated files.
6. Set needsUserReview true unless all actions are risk=low and grounded in a matched case.
7. confidence 0.0–1.0; lower if no KB match or ambiguous stacktrace.
8. Do not invent Modrinth project IDs; omit projectId if unknown (launcher resolves by modId).
9. Never invent version numbers or file paths. If the exact version is unknown, set "version" and "path" to null (launcher resolves). Do not use placeholders like 1.2.3, 0.0.1, or /game/mods/….
10. If a mod is a suspected culprit / already installed, prefer disable_mod or remove_mod — never install_mod for that mod.
11. Return JSON only. No markdown fences."#;

/// Post-resolution distill: compress a user's trial-and-error fix path into a
/// minimal ActionPlan suitable for sharing as an ExperienceCapsule.
pub const DISTILL_SYSTEM_PROMPT: &str = r#"You are TuffBox Crash Fix Distiller. You only output ONE JSON object matching schemaVersion 1.
The crash was already fixed by the user. Your job is to distill their action history into the MINIMAL efficient ActionPlan that peers should apply for the same fingerprint.

Rules:
1. Drop dead ends, redundant disables, and trial steps that were later undone or superseded.
2. Keep only actions that were necessary for the successful outcome.
3. Prefer the smallest set of ops; prefer disable_mod over remove_mod when either worked.
4. humanExplanation must briefly state the root cause and the efficient fix (no raw logs).
5. Set source to "distill", needsUserReview to true (beta human confirm before network share).
6. confidence 0.0–1.0 based on how clear the causal path is.
7. Every mutating action MUST include modId (except pure edit_config).
8. Return JSON only. No markdown fences."#;

pub const ACTION_PLAN_JSON_SCHEMA_HINT: &str = r#"Return ONLY valid JSON with this schema:
{
  "schemaVersion": 1,
  "humanExplanation": string,
  "confidence": number (0.0-1.0),
  "suspectedMods": string[],
  "needsUserReview": boolean,
  "source": "kb"|"ai"|"hybrid"|null,
  "matchedCaseIds": string[]|null,
  "actions": [{
    "op": "install_mod"|"remove_mod"|"disable_mod"|"update_mod"|"change_mod_version"|"reinstall_mod"|"edit_config",
    "modId": string|null,
    "provider": "modrinth"|"curseforge"|null,
    "projectId": string|null,
    "version": string|null,
    "path": string|null,
    "patchType": "json_merge"|"toml_set"|"properties_set"|"replace_file"|null,
    "patch": object|string|null,
    "reason": string,
    "risk": "low"|"medium"|"high"
  }],
  "additionalContext": string|null
}"#;

/// Allowed `op` values.
pub const KNOWN_OPS: &[&str] = &[
    "install_mod",
    "remove_mod",
    "disable_mod",
    "update_mod",
    "change_mod_version",
    "reinstall_mod",
    "edit_config",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ActionPlan {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub human_explanation: String,
    #[serde(default = "default_confidence")]
    pub confidence: f64,
    #[serde(default)]
    pub suspected_mods: Vec<String>,
    #[serde(default = "default_true")]
    pub needs_user_review: bool,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub matched_case_ids: Vec<String>,
    #[serde(default)]
    pub actions: Vec<LauncherAction>,
    #[serde(default)]
    pub additional_context: Option<String>,
}

fn default_schema_version() -> u32 {
    ACTION_PLAN_SCHEMA_VERSION
}
fn default_confidence() -> f64 {
    0.5
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LauncherAction {
    pub op: String,
    #[serde(default)]
    pub mod_id: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub patch_type: Option<String>,
    #[serde(default)]
    pub patch: Option<Value>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default = "default_risk")]
    pub risk: String,
}

fn default_risk() -> String {
    "medium".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionPlanValidation {
    pub ok: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Diagnose transport mode: server AI (default), local AI, or KB-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum DiagnoseMode {
    #[default]
    Server,
    Local,
    KbOnly,
}

impl DiagnoseMode {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "local" => Self::Local,
            "kb_only" | "kb-only" | "kbonly" => Self::KbOnly,
            _ => Self::Server,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::Local => "local",
            Self::KbOnly => "kb_only",
        }
    }
}

/// Parse ActionPlan JSON. Also accepts legacy CrashAiResponse shape and normalizes it.
pub fn parse_action_plan(json_str: &str) -> Result<ActionPlan, String> {
    let trimmed = strip_fences(json_str);
    let v: Value =
        serde_json::from_str(trimmed).map_err(|e| format!("Invalid ActionPlan JSON: {e}"))?;
    parse_action_plan_value(&v)
}

pub fn parse_action_plan_value(v: &Value) -> Result<ActionPlan, String> {
    // Prefer new schema when `actions` with `op` is present.
    if v.get("actions").and_then(|a| a.as_array()).is_some()
        && v.get("actions")
            .and_then(|a| a.as_array())
            .and_then(|a| a.first())
            .map(|first| first.get("op").is_some())
            .unwrap_or(true)
    {
        if let Ok(plan) = serde_json::from_value::<ActionPlan>(v.clone()) {
            return Ok(normalize_plan(plan));
        }
    }

    // Legacy: recommended_actions / recommendedActions with action_type
    let human = str_field(v, &["humanExplanation", "human_explanation"])
        .unwrap_or_else(|| "No explanation provided.".into());
    let confidence = v
        .get("confidence")
        .and_then(|c| c.as_f64())
        .unwrap_or(0.5)
        .clamp(0.0, 1.0);
    let suspected = string_array(v, &["suspectedMods", "suspected_mods"]);
    let needs_review = v
        .get("needsUserReview")
        .or_else(|| v.get("needs_user_review"))
        .and_then(|b| b.as_bool())
        .unwrap_or(true);
    let source = str_field(v, &["source"]);
    let matched = string_array(v, &["matchedCaseIds", "matched_case_ids"]);
    let additional = str_field(v, &["additionalContext", "additional_context"]);

    let legacy_actions = v
        .get("actions")
        .or_else(|| v.get("recommended_actions"))
        .or_else(|| v.get("recommendedActions"))
        .and_then(|a| a.as_array())
        .cloned()
        .unwrap_or_default();

    let mut actions = Vec::new();
    for act in legacy_actions {
        if let Some(op) = act.get("op").and_then(|s| s.as_str()) {
            actions.push(launcher_action_from_value(&act, op)?);
            continue;
        }
        let action_type = act
            .get("action_type")
            .or_else(|| act.get("actionType"))
            .and_then(|s| s.as_str())
            .unwrap_or("unknown");
        let op = legacy_action_type_to_op(action_type);
        let mut la = launcher_action_from_value(&act, &op)?;
        if la.reason.is_none() {
            la.reason = act
                .get("description")
                .and_then(|s| s.as_str())
                .map(|s| s.to_string());
        }
        actions.push(la);
    }

    Ok(normalize_plan(ActionPlan {
        schema_version: v
            .get("schemaVersion")
            .or_else(|| v.get("schema_version"))
            .and_then(|n| n.as_u64())
            .map(|n| n as u32)
            .unwrap_or(ACTION_PLAN_SCHEMA_VERSION),
        human_explanation: human,
        confidence,
        suspected_mods: suspected,
        needs_user_review: needs_review,
        source,
        matched_case_ids: matched,
        actions,
        additional_context: additional,
    }))
}

fn launcher_action_from_value(act: &Value, op: &str) -> Result<LauncherAction, String> {
    Ok(LauncherAction {
        op: op.to_string(),
        mod_id: str_field(act, &["modId", "mod_id"]),
        provider: str_field(act, &["provider"]),
        project_id: str_field(act, &["projectId", "project_id"]),
        version: str_field(act, &["version", "targetVersion", "target_version"]),
        path: str_field(act, &["path"]),
        patch_type: str_field(act, &["patchType", "patch_type"]),
        patch: act.get("patch").cloned(),
        reason: str_field(act, &["reason", "description"]),
        risk: str_field(act, &["risk"]).unwrap_or_else(|| "medium".into()),
    })
}

fn legacy_action_type_to_op(action_type: &str) -> String {
    match action_type.trim().to_ascii_lowercase().as_str() {
        "update" | "update_mod" => "update_mod".into(),
        "remove" | "remove_mod" => "remove_mod".into(),
        "install" | "install_mod" => "install_mod".into(),
        "disable" | "disable_mod" => "disable_mod".into(),
        "config_change" | "edit_config" | "config" => "edit_config".into(),
        "reinstall" | "reinstall_mod" => "reinstall_mod".into(),
        "change_mod_version" | "change_version" => "change_mod_version".into(),
        other => other.to_string(),
    }
}

fn normalize_plan(plan: ActionPlan) -> ActionPlan {
    ground_action_plan(plan, &[], &[]).plan
}

/// Result of polarity / inventory grounding after LLM or KB parse.
#[derive(Debug, Clone)]
pub struct GroundingResult {
    pub plan: ActionPlan,
    /// Human-readable rewrite notes for UI (e.g. install→disable).
    pub notes: Vec<String>,
}

/// Normalize placeholders / polarity, then ground against inventory + missing deps.
pub fn ground_action_plan(
    mut plan: ActionPlan,
    inventory_mod_ids: &[String],
    missing_dep_ids: &[String],
) -> GroundingResult {
    let mut notes = Vec::new();
    plan.confidence = plan.confidence.clamp(0.0, 1.0);
    if plan.schema_version == 0 {
        plan.schema_version = ACTION_PLAN_SCHEMA_VERSION;
    }
    let suspected: Vec<String> = plan
        .suspected_mods
        .iter()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    let inventory_l: Vec<String> = inventory_mod_ids
        .iter()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    let missing_l: Vec<String> = missing_dep_ids
        .iter()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    let has_inventory = !inventory_l.is_empty();

    let mut kept = Vec::with_capacity(plan.actions.len());
    for mut a in plan.actions.drain(..) {
        a.op = legacy_action_type_to_op(&a.op);
        if a.risk.is_empty() {
            a.risk = "medium".into();
        }
        if is_placeholder_version(a.version.as_deref()) {
            notes.push(format!(
                "stripped placeholder version on {}",
                a.mod_id.as_deref().unwrap_or(&a.op)
            ));
            a.version = None;
        }
        if is_invented_mod_path(a.path.as_deref()) {
            notes.push(format!(
                "stripped invented path on {}",
                a.mod_id.as_deref().unwrap_or(&a.op)
            ));
            a.path = None;
        }

        let id_l = a
            .mod_id
            .as_deref()
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty());

        if a.op == "install_mod" {
            if let Some(ref id) = id_l {
                if suspected.iter().any(|s| s == id) {
                    a.op = "disable_mod".into();
                    let note = "rewrote install→disable: mod is a suspected culprit";
                    notes.push(format!("{id}: {note}"));
                    a.reason = Some(match a.reason.take() {
                        Some(r) if !r.is_empty() => format!("{r} ({note})"),
                        _ => note.into(),
                    });
                    if a.risk.eq_ignore_ascii_case("low") {
                        a.risk = "medium".into();
                    }
                } else if has_inventory && inventory_l.iter().any(|s| s == id) {
                    // Already installed and not a suspect → reinstall instead of install.
                    a.op = "reinstall_mod".into();
                    let note = "rewrote install→reinstall: mod already in inventory";
                    notes.push(format!("{id}: {note}"));
                    a.reason = Some(match a.reason.take() {
                        Some(r) if !r.is_empty() => format!("{r} ({note})"),
                        _ => note.into(),
                    });
                }
            }
        }

        // Drop mutating mod ops that reference unknown ids (not installed, not missing dep).
        if has_inventory
            && matches!(
                a.op.as_str(),
                "install_mod"
                    | "remove_mod"
                    | "disable_mod"
                    | "update_mod"
                    | "change_mod_version"
                    | "reinstall_mod"
            )
        {
            if let Some(ref id) = id_l {
                let in_inv = inventory_l.iter().any(|s| s == id);
                let in_missing = missing_l.iter().any(|s| s == id);
                let in_suspect = suspected.iter().any(|s| s == id);
                let allowed = match a.op.as_str() {
                    "install_mod" => {
                        if missing_l.is_empty() {
                            // No explicit missing list → keep installs of mods not already present
                            // (validate will warn). Drop only if somehow still marked installed.
                            !in_inv
                        } else {
                            in_missing
                        }
                    }
                    _ => in_inv || in_suspect,
                };
                if !allowed {
                    notes.push(format!(
                        "dropped {}:{} — not in inventory / missing deps",
                        a.op, id
                    ));
                    continue;
                }
            }
        }

        kept.push(a);
    }
    plan.actions = kept;
    GroundingResult { plan, notes }
}

/// Overlay high-signal Crash Assistant findings onto a weak / conflicting AI plan.
pub fn overlay_crash_assistant_findings(
    mut plan: ActionPlan,
    findings: &[crate::ai_explanation::CrashAiFinding],
) -> ActionPlan {
    const JAVA_CODES: &[&str] = &[
        "UNSUPPORTED_CLASS_VERSION",
        "JAVA_VERSION_MISMATCH",
        "WRONG_JAVA_VERSION",
    ];
    let java = findings
        .iter()
        .find(|f| JAVA_CODES.iter().any(|c| f.code.eq_ignore_ascii_case(c)));
    let Some(java) = java else {
        return plan;
    };

    let auto = java
        .auto_fix
        .clone()
        .unwrap_or_else(|| java.description.clone());
    let java_note = format!(
        "Crash Assistant [{}]: {} — {}",
        java.code, java.title, auto
    );

    let only_mod_churn = !plan.actions.is_empty()
        && plan.actions.iter().all(|a| {
            matches!(
                a.op.as_str(),
                "install_mod" | "remove_mod" | "disable_mod" | "update_mod" | "reinstall_mod"
            )
        });
    if only_mod_churn {
        plan.needs_user_review = true;
        plan.confidence = plan.confidence.min(0.55);
        let ctx = match plan.additional_context.take() {
            Some(c) if !c.is_empty() => format!("{c}\n{java_note}"),
            _ => java_note.clone(),
        };
        plan.additional_context = Some(ctx);
        if !plan.human_explanation.to_ascii_lowercase().contains("java") {
            plan.human_explanation = format!(
                "{}\n\nAlso check Java: {} ({})",
                plan.human_explanation.trim(),
                java.title,
                auto
            );
        }
    } else {
        let ctx = match plan.additional_context.take() {
            Some(c) if !c.is_empty() => format!("{c}\n{java_note}"),
            _ => java_note,
        };
        plan.additional_context = Some(ctx);
    }
    plan
}

fn is_placeholder_version(version: Option<&str>) -> bool {
    let Some(v) = version.map(str::trim).filter(|s| !s.is_empty()) else {
        return false;
    };
    matches!(
        v,
        "1.2.3"
            | "0.0.0"
            | "x.y.z"
            | "X.Y.Z"
            | "latest"
            | "VERSION"
            | "version"
            | "<version>"
            | "{{version}}"
    ) || v.eq_ignore_ascii_case("null")
        || v.eq_ignore_ascii_case("unknown")
        || v.eq_ignore_ascii_case("example")
        || v.eq_ignore_ascii_case("string")
}

/// Paths like `/game/mods/foo/1.2.3` are not instance-relative and come from model fluff.
fn is_invented_mod_path(path: Option<&str>) -> bool {
    let Some(p) = path.map(str::trim).filter(|s| !s.is_empty()) else {
        return false;
    };
    let lower = p.to_ascii_lowercase();
    if lower.starts_with("/game/") || lower.starts_with("game/mods/") {
        return true;
    }
    // Absolute-looking invent: mods/<id>/<placeholder-version>
    let segs: Vec<&str> = p.split(['/', '\\']).filter(|s| !s.is_empty()).collect();
    segs.len() >= 3
        && segs.iter().any(|s| s.eq_ignore_ascii_case("mods"))
        && segs.iter().any(|s| is_placeholder_version(Some(s)))
}

/// Structural validation before apply. Unknown ops are errors (not applied).
pub fn validate_action_plan(plan: &ActionPlan) -> ActionPlanValidation {
    validate_action_plan_with_inventory(plan, &[], &[])
}

/// Like [`validate_action_plan`], plus inventory grounding warnings/errors.
pub fn validate_action_plan_with_inventory(
    plan: &ActionPlan,
    inventory_mod_ids: &[String],
    missing_dep_ids: &[String],
) -> ActionPlanValidation {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    if plan.human_explanation.trim().is_empty() {
        warnings.push("humanExplanation is empty".into());
    }
    if plan.actions.is_empty() {
        warnings.push("actions array is empty".into());
    }

    let inventory_l: Vec<String> = inventory_mod_ids
        .iter()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    let missing_l: Vec<String> = missing_dep_ids
        .iter()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    let has_inventory = !inventory_l.is_empty();

    for (i, a) in plan.actions.iter().enumerate() {
        let label = format!("actions[{i}]");
        if !KNOWN_OPS.contains(&a.op.as_str()) {
            errors.push(format!("{label}: unknown op '{}'", a.op));
            continue;
        }
        match a.op.as_str() {
            "install_mod" | "remove_mod" | "disable_mod" | "update_mod" | "change_mod_version"
            | "reinstall_mod" => {
                let id = a
                    .mod_id
                    .as_deref()
                    .or(a.project_id.as_deref())
                    .unwrap_or("")
                    .trim();
                if id.is_empty() {
                    errors.push(format!("{label}: {} requires modId or projectId", a.op));
                } else if has_inventory {
                    let id_l = id.to_ascii_lowercase();
                    let in_inv = inventory_l.iter().any(|s| s == &id_l);
                    let in_missing = missing_l.iter().any(|s| s == &id_l);
                    match a.op.as_str() {
                        "install_mod" => {
                            if !in_missing && !missing_l.is_empty() {
                                errors.push(format!(
                                    "{label}: install_mod '{id}' is not an explicit missing dependency"
                                ));
                            } else if !in_missing && missing_l.is_empty() && !in_inv {
                                warnings.push(format!(
                                    "{label}: install_mod '{id}' not listed as a missing dependency — verify before apply"
                                ));
                            }
                        }
                        "update_mod" | "change_mod_version" => {
                            if !in_inv {
                                errors.push(format!(
                                    "{label}: {} '{id}' is not in project inventory",
                                    a.op
                                ));
                            }
                            if a.op == "update_mod"
                                && a.version.as_deref().unwrap_or("").trim().is_empty()
                            {
                                warnings.push(format!(
                                    "{label}: update_mod has no version (launcher will pick latest)"
                                ));
                            }
                        }
                        _ => {
                            if !in_inv {
                                warnings.push(format!(
                                    "{label}: {} '{id}' may not be in project inventory",
                                    a.op
                                ));
                            }
                        }
                    }
                }
                if a.op == "change_mod_version"
                    && a.version.as_deref().unwrap_or("").trim().is_empty()
                {
                    errors.push(format!("{label}: change_mod_version requires version"));
                }
            }
            "edit_config" => {
                if a.path.as_deref().unwrap_or("").trim().is_empty() {
                    errors.push(format!("{label}: edit_config requires path"));
                }
                let pt = a.patch_type.as_deref().unwrap_or("replace_file");
                if !matches!(
                    pt,
                    "json_merge" | "toml_set" | "properties_set" | "replace_file"
                ) {
                    errors.push(format!("{label}: unknown patchType '{pt}'"));
                }
                if a.patch.is_none() {
                    errors.push(format!("{label}: edit_config requires patch"));
                }
            }
            _ => {}
        }
        match a.risk.to_ascii_lowercase().as_str() {
            "low" | "medium" | "high" => {}
            other => warnings.push(format!("{label}: unusual risk '{other}'")),
        }
    }

    ActionPlanValidation {
        ok: errors.is_empty(),
        errors,
        warnings,
    }
}

/// Map a single launcher action to Crash Assistant FixAction (mod ops only).
pub fn launcher_action_to_fix_action(action: &LauncherAction) -> Option<FixAction> {
    let mod_id = action
        .mod_id
        .clone()
        .or_else(|| action.project_id.clone())
        .filter(|s| !s.trim().is_empty());
    let kind = match action.op.as_str() {
        "disable_mod" => "disableMod",
        "remove_mod" => "removeMod",
        "reinstall_mod" => "reinstallMod",
        "update_mod" | "change_mod_version" => "updateMod",
        "install_mod" => "installDependency",
        _ => return None,
    };
    let label = action
        .reason
        .clone()
        .unwrap_or_else(|| format!("{} {}", action.op, mod_id.as_deref().unwrap_or("")));
    Some(FixAction {
        kind: kind.into(),
        label,
        mod_id,
    })
}

/// Map launcher actions to ChangePlan (includes edit_config).
pub fn action_plan_to_change_plan(plan: &ActionPlan) -> ChangePlan {
    let mut actions = Vec::new();
    let mut max_risk = ChangeRisk::Low;
    for a in &plan.actions {
        let risk = match a.risk.to_ascii_lowercase().as_str() {
            "high" => ChangeRisk::High,
            "medium" => ChangeRisk::Medium,
            _ => ChangeRisk::Low,
        };
        if risk_rank(&risk) > risk_rank(&max_risk) {
            max_risk = risk;
        }
        match a.op.as_str() {
            "install_mod" => {
                let project_id = a
                    .project_id
                    .clone()
                    .or_else(|| a.mod_id.clone())
                    .unwrap_or_default();
                actions.push(ChangeAction::InstallMod {
                    project_id,
                    version: a.version.clone(),
                });
            }
            "remove_mod" => {
                if let Some(id) = a.mod_id.as_deref() {
                    actions.push(ChangeAction::RemoveMod {
                        node_id: NodeId(format!("mod:{id}")),
                    });
                }
            }
            "disable_mod" => {
                if let Some(id) = a.mod_id.as_deref() {
                    actions.push(ChangeAction::DisableMod {
                        node_id: NodeId(format!("mod:{id}")),
                    });
                }
            }
            "update_mod" | "change_mod_version" | "reinstall_mod" => {
                if let Some(id) = a.mod_id.as_deref() {
                    let target = a
                        .version
                        .clone()
                        .unwrap_or_else(|| "latest-compatible".into());
                    actions.push(ChangeAction::UpdateMod {
                        node_id: NodeId(format!("mod:{id}")),
                        target_version: target,
                    });
                }
            }
            "edit_config" => {
                if let Some(path) = a.path.clone() {
                    let patch = encode_edit_config_patch(a);
                    actions.push(ChangeAction::EditConfig { path, patch });
                }
            }
            _ => {}
        }
    }
    ChangePlan {
        summary: plan.human_explanation.clone(),
        risk: max_risk,
        actions,
        requires_snapshot: true,
    }
}

/// Encode patch metadata into the ChangeAction::EditConfig.patch string (JSON envelope).
pub fn encode_edit_config_patch(action: &LauncherAction) -> String {
    let envelope = serde_json::json!({
        "patchType": action.patch_type.as_deref().unwrap_or("replace_file"),
        "patch": action.patch.clone().unwrap_or(Value::Null),
        "reason": action.reason,
    });
    envelope.to_string()
}

/// Apply a config patch to file contents. Pure function for EditConfig apply.
pub fn apply_config_patch(
    current: &str,
    relative_path: &str,
    patch_type: &str,
    patch: &Value,
) -> Result<String, String> {
    match patch_type {
        "replace_file" => {
            if let Some(s) = patch.as_str() {
                return Ok(s.to_string());
            }
            if let Some(s) = patch.get("content").and_then(|v| v.as_str()) {
                return Ok(s.to_string());
            }
            Err("replace_file patch must be a string or {content: string}".into())
        }
        "json_merge" => {
            let mut base: Value = if current.trim().is_empty() {
                Value::Object(Default::default())
            } else {
                serde_json::from_str(current).map_err(|e| format!("invalid JSON config: {e}"))?
            };
            merge_json(&mut base, patch)?;
            Ok(serde_json::to_string_pretty(&base).map_err(|e| e.to_string())?)
        }
        "toml_set" => apply_toml_set(current, patch),
        "properties_set" => apply_properties_set(current, patch),
        other => {
            // Infer from extension if caller passed a generic type.
            let ext = relative_path
                .rsplit('.')
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            match (other, ext.as_str()) {
                (_, "json" | "json5") => apply_config_patch(current, relative_path, "json_merge", patch),
                (_, "toml") => apply_config_patch(current, relative_path, "toml_set", patch),
                (_, "properties" | "cfg") => {
                    apply_config_patch(current, relative_path, "properties_set", patch)
                }
                _ => Err(format!("unsupported patchType '{other}'")),
            }
        }
    }
}

fn apply_toml_set(current: &str, patch: &Value) -> Result<String, String> {
    let mut doc: toml::Value = if current.trim().is_empty() {
        toml::Value::Table(toml::map::Map::new())
    } else {
        current
            .parse()
            .map_err(|e| format!("invalid TOML config: {e}"))?
    };
    let obj = patch
        .as_object()
        .ok_or_else(|| "toml_set patch must be a JSON object of key→value".to_string())?;
    for (key, val) in obj {
        set_toml_path(&mut doc, key, json_to_toml(val)?)?;
    }
    Ok(toml::to_string_pretty(&doc).map_err(|e| e.to_string())?)
}

fn apply_properties_set(current: &str, patch: &Value) -> Result<String, String> {
    let mut props = crate::properties_parser::PropertiesFile::parse(current);
    let obj = patch
        .as_object()
        .ok_or_else(|| "properties_set patch must be a JSON object of key→value".to_string())?;
    for (key, val) in obj {
        let s = match val {
            Value::String(s) => s.clone(),
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => n.to_string(),
            other => other.to_string(),
        };
        props.set(key, &s);
    }
    Ok(props.to_string())
}

fn set_toml_path(doc: &mut toml::Value, path: &str, value: toml::Value) -> Result<(), String> {
    let parts: Vec<&str> = path.split('.').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return Err("empty TOML key path".into());
    }
    let mut cur = doc;
    for (i, part) in parts.iter().enumerate() {
        if i + 1 == parts.len() {
            match cur {
                toml::Value::Table(t) => {
                    t.insert((*part).to_string(), value);
                    return Ok(());
                }
                _ => return Err(format!("cannot set '{path}': parent is not a table")),
            }
        }
        if !matches!(cur, toml::Value::Table(_)) {
            return Err(format!("cannot set '{path}': parent is not a table"));
        }
        let table = cur.as_table_mut().unwrap();
        if !table.contains_key(*part) {
            table.insert((*part).to_string(), toml::Value::Table(toml::map::Map::new()));
        }
        cur = table.get_mut(*part).unwrap();
    }
    Ok(())
}

fn json_to_toml(v: &Value) -> Result<toml::Value, String> {
    match v {
        Value::Null => Ok(toml::Value::String(String::new())),
        Value::Bool(b) => Ok(toml::Value::Boolean(*b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(toml::Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(toml::Value::Float(f))
            } else {
                Err("unsupported number".into())
            }
        }
        Value::String(s) => Ok(toml::Value::String(s.clone())),
        Value::Array(arr) => {
            let mut out = Vec::new();
            for item in arr {
                out.push(json_to_toml(item)?);
            }
            Ok(toml::Value::Array(out))
        }
        Value::Object(map) => {
            let mut table = toml::map::Map::new();
            for (k, val) in map {
                table.insert(k.clone(), json_to_toml(val)?);
            }
            Ok(toml::Value::Table(table))
        }
    }
}

fn merge_json(base: &mut Value, patch: &Value) -> Result<(), String> {
    match (base, patch) {
        (Value::Object(base_map), Value::Object(patch_map)) => {
            for (k, v) in patch_map {
                if let Some(existing) = base_map.get_mut(k) {
                    if existing.is_object() && v.is_object() {
                        merge_json(existing, v)?;
                    } else {
                        *existing = v.clone();
                    }
                } else {
                    base_map.insert(k.clone(), v.clone());
                }
            }
            Ok(())
        }
        (base, patch) => {
            *base = patch.clone();
            Ok(())
        }
    }
}

fn risk_rank(r: &ChangeRisk) -> u8 {
    match r {
        ChangeRisk::Low => 0,
        ChangeRisk::Medium => 1,
        ChangeRisk::High => 2,
    }
}

/// Build an ActionPlan from a strong KB hit (kb_only mode).
pub fn plan_from_kb_hit(
    solution: &str,
    suspected_mods: &[String],
    actions: &[AiAction],
    case_id: &str,
    score: f64,
) -> ActionPlan {
    let launcher_actions: Vec<LauncherAction> = actions
        .iter()
        .map(|a| LauncherAction {
            op: legacy_action_type_to_op(&a.action_type),
            mod_id: a.mod_id.clone(),
            provider: None,
            project_id: None,
            version: None,
            path: None,
            patch_type: None,
            patch: None,
            reason: Some(a.description.clone()),
            risk: a.risk.clone(),
        })
        .collect();
    plan_from_launcher_actions(solution, suspected_mods, launcher_actions, case_id, score)
}

/// Build an ActionPlan from already-executable launcher actions (remote KB hits).
pub fn plan_from_launcher_actions(
    solution: &str,
    suspected_mods: &[String],
    actions: Vec<LauncherAction>,
    case_id: &str,
    score: f64,
) -> ActionPlan {
    ActionPlan {
        schema_version: ACTION_PLAN_SCHEMA_VERSION,
        human_explanation: solution.to_string(),
        confidence: score.clamp(0.0, 1.0),
        suspected_mods: suspected_mods.to_vec(),
        needs_user_review: score < 0.9
            || actions
                .iter()
                .any(|a| a.risk.eq_ignore_ascii_case("high")),
        source: Some("kb".into()),
        matched_case_ids: vec![case_id.to_string()],
        actions,
        additional_context: None,
    }
}

/// Convert ActionPlan → legacy AiAction list (for feedback / old UI).
pub fn plan_to_legacy_ai_actions(plan: &ActionPlan) -> Vec<AiAction> {
    plan.actions
        .iter()
        .map(|a| AiAction {
            action_type: match a.op.as_str() {
                "install_mod" => "install".into(),
                "remove_mod" => "remove".into(),
                "disable_mod" => "disable".into(),
                "update_mod" | "change_mod_version" => "update".into(),
                "edit_config" => "config_change".into(),
                "reinstall_mod" => "update".into(),
                other => other.into(),
            },
            mod_id: a.mod_id.clone(),
            description: a.reason.clone().unwrap_or_default(),
            risk: a.risk.clone(),
        })
        .collect()
}

fn strip_fences(json_str: &str) -> &str {
    json_str
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
}

fn str_field(v: &Value, keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Some(s) = v.get(*k).and_then(|x| x.as_str()) {
            return Some(s.to_string());
        }
    }
    None
}

fn string_array(v: &Value, keys: &[&str]) -> Vec<String> {
    for k in keys {
        if let Some(arr) = v.get(*k).and_then(|x| x.as_array()) {
            return arr
                .iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect();
        }
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_new_schema() {
        let json = r#"{
          "schemaVersion": 1,
          "humanExplanation": "Missing Indium",
          "confidence": 0.9,
          "suspectedMods": ["sodium"],
          "needsUserReview": true,
          "source": "hybrid",
          "matchedCaseIds": ["builtin-noclassdef"],
          "actions": [
            {"op":"install_mod","modId":"indium","reason":"Install Indium","risk":"low"}
          ]
        }"#;
        let plan = parse_action_plan(json).unwrap();
        assert_eq!(plan.actions.len(), 1);
        assert_eq!(plan.actions[0].op, "install_mod");
        let v = validate_action_plan(&plan);
        assert!(v.ok, "{:?}", v.errors);
    }

    #[test]
    fn parses_legacy_schema() {
        let json = r#"{"human_explanation":"Test","confidence":0.8,"suspected_mods":["sodium"],"recommended_actions":[{"action_type":"install","mod_id":"indium","description":"Install Indium","risk":"low"}],"needs_user_review":true}"#;
        let plan = parse_action_plan(json).unwrap();
        assert_eq!(plan.actions[0].op, "install_mod");
        assert_eq!(plan.actions[0].mod_id.as_deref(), Some("indium"));
    }

    #[test]
    fn strips_placeholder_version_and_flips_install_on_suspect() {
        let json = r#"{
          "schemaVersion": 1,
          "humanExplanation": "Critters crashed",
          "confidence": 0.7,
          "suspectedMods": ["crittersandcompanions"],
          "needsUserReview": true,
          "actions": [
            {
              "op":"install_mod",
              "modId":"crittersandcompanions",
              "version":"1.2.3",
              "path":"/game/mods/crittersandcompanions/1.2.3",
              "reason":"Install fix",
              "risk":"low"
            }
          ]
        }"#;
        let plan = parse_action_plan(json).unwrap();
        assert_eq!(plan.actions[0].op, "disable_mod");
        assert_eq!(plan.actions[0].version, None);
        assert_eq!(plan.actions[0].path, None);
        assert!(plan.actions[0]
            .reason
            .as_deref()
            .unwrap_or("")
            .contains("install→disable"));
    }

    #[test]
    fn grounds_install_missing_indium_and_drops_invented() {
        let json = r#"{
          "schemaVersion": 1,
          "humanExplanation": "Missing Indium",
          "confidence": 0.9,
          "suspectedMods": ["sodium"],
          "needsUserReview": true,
          "actions": [
            {"op":"install_mod","modId":"indium","reason":"Install Indium","risk":"low"},
            {"op":"install_mod","modId":"madeupmod","reason":"Invented","risk":"low"}
          ]
        }"#;
        let plan = parse_action_plan(json).unwrap();
        let grounded = ground_action_plan(
            plan,
            &["sodium".into(), "fabric-api".into()],
            &["indium".into()],
        );
        assert_eq!(grounded.plan.actions.len(), 1);
        assert_eq!(grounded.plan.actions[0].mod_id.as_deref(), Some("indium"));
        assert_eq!(grounded.plan.actions[0].op, "install_mod");
        assert!(grounded.notes.iter().any(|n| n.contains("madeupmod")));
        let v = validate_action_plan_with_inventory(
            &grounded.plan,
            &["sodium".into(), "fabric-api".into()],
            &["indium".into()],
        );
        assert!(v.ok, "{:?}", v.errors);
    }

    #[test]
    fn overlays_java_finding_on_mod_only_plan() {
        let plan = ActionPlan {
            schema_version: 1,
            human_explanation: "Mod X is broken".into(),
            confidence: 0.8,
            suspected_mods: vec!["x".into()],
            needs_user_review: false,
            source: Some("ai".into()),
            matched_case_ids: vec![],
            actions: vec![LauncherAction {
                op: "disable_mod".into(),
                mod_id: Some("x".into()),
                provider: None,
                project_id: None,
                version: None,
                path: None,
                patch_type: None,
                patch: None,
                reason: Some("disable".into()),
                risk: "medium".into(),
            }],
            additional_context: None,
        };
        let findings = vec![crate::ai_explanation::CrashAiFinding {
            code: "UNSUPPORTED_CLASS_VERSION".into(),
            title: "Needs Java 24".into(),
            description: "class 68".into(),
            auto_fix: Some("Install Java 24+".into()),
        }];
        let out = overlay_crash_assistant_findings(plan, &findings);
        assert!(out.needs_user_review);
        assert!(out.confidence <= 0.55);
        assert!(out.human_explanation.to_ascii_lowercase().contains("java"));
        assert!(out
            .additional_context
            .as_deref()
            .unwrap_or("")
            .contains("UNSUPPORTED_CLASS_VERSION"));
    }

    #[test]
    fn rejects_unknown_op() {
        let plan = ActionPlan {
            schema_version: 1,
            human_explanation: "x".into(),
            confidence: 0.5,
            suspected_mods: vec![],
            needs_user_review: true,
            source: None,
            matched_case_ids: vec![],
            actions: vec![LauncherAction {
                op: "delete_world".into(),
                mod_id: None,
                provider: None,
                project_id: None,
                version: None,
                path: None,
                patch_type: None,
                patch: None,
                reason: None,
                risk: "high".into(),
            }],
            additional_context: None,
        };
        let v = validate_action_plan(&plan);
        assert!(!v.ok);
    }

    #[test]
    fn json_merge_patch() {
        let current = r#"{"a":1,"b":{"c":2}}"#;
        let patch = serde_json::json!({"b":{"d":3},"e":4});
        let out = apply_config_patch(current, "config/x.json", "json_merge", &patch).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["a"], 1);
        assert_eq!(v["b"]["c"], 2);
        assert_eq!(v["b"]["d"], 3);
        assert_eq!(v["e"], 4);
    }

    #[test]
    fn toml_set_patch() {
        let current = "foo = 1\n[bar]\nbaz = true\n";
        let patch = serde_json::json!({"bar.baz": false, "qux": "hi"});
        let out = apply_config_patch(current, "config/x.toml", "toml_set", &patch).unwrap();
        assert!(out.contains("baz = false") || out.contains("baz=false"));
        assert!(out.contains("qux"));
    }

    #[test]
    fn maps_to_fix_action() {
        let a = LauncherAction {
            op: "disable_mod".into(),
            mod_id: Some("oculus".into()),
            provider: None,
            project_id: None,
            version: None,
            path: None,
            patch_type: None,
            patch: None,
            reason: Some("Conflict".into()),
            risk: "low".into(),
        };
        let f = launcher_action_to_fix_action(&a).unwrap();
        assert_eq!(f.kind, "disableMod");
        assert_eq!(f.mod_id.as_deref(), Some("oculus"));
    }
}
