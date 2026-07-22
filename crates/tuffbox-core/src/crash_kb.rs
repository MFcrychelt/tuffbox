//! Local knowledge base of known crash fingerprints and solutions.
//!
//! Used by AI Explain (RAG): retrieve similar past cases and inject them into
//! the LLM prompt. No network calls — seed is builtin; user feedback is JSONL.

use crate::action_plan::LauncherAction;
use crate::ai_explanation::AiAction;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashFingerprint {
    pub exception: String,
    pub frames: Vec<String>,
    pub mod_file: Option<String>,
    pub mixin: Option<String>,
    pub mc_major: String,
    pub loader: String,
    /// Compact key for exact match / display.
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashCase {
    pub id: String,
    pub fingerprint: CrashFingerprint,
    #[serde(default)]
    pub symptoms: Vec<String>,
    #[serde(default)]
    pub suspected_mods: Vec<String>,
    pub solution: String,
    /// Legacy advisory actions (kept for older JSONL + prompt RAG).
    #[serde(default)]
    pub actions: Vec<AiAction>,
    /// Executable ActionPlan ops (preferred when present).
    #[serde(default)]
    pub launcher_actions: Vec<LauncherAction>,
    /// Author-only notes — never send to launcher clients / remote lookup responses.
    #[serde(default)]
    pub notes: Option<String>,
    /// `builtin` | `user_feedback` | `imported` | `authored`
    pub source: String,
    #[serde(default)]
    pub success_count: u32,
    #[serde(default)]
    pub fail_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimilarCaseHit {
    pub id: String,
    pub score: f64,
    pub solution: String,
    pub suspected_mods: Vec<String>,
    pub actions: Vec<AiAction>,
    pub fingerprint_key: String,
    pub source: String,
}

pub fn user_kb_path(project_dir: &Path) -> PathBuf {
    project_dir.join(".tuffbox").join("crash_kb").join("cases.jsonl")
}

pub fn author_export_dir(project_dir: &Path) -> PathBuf {
    project_dir.join(".tuffbox").join("crash_kb").join("export")
}

/// Payload for authoring a private KB case from a crash + resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorCaseInput {
    #[serde(default)]
    pub id: Option<String>,
    pub fingerprint: CrashFingerprint,
    pub solution: String,
    #[serde(default)]
    pub symptoms: Vec<String>,
    #[serde(default)]
    pub suspected_mods: Vec<String>,
    #[serde(default)]
    pub launcher_actions: Vec<LauncherAction>,
    /// Also accept legacy AiAction list; converted to launcher ops when needed.
    #[serde(default)]
    pub actions: Vec<AiAction>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorCaseSaveResult {
    pub case_id: String,
    pub kb_path: String,
    pub export_path: String,
    pub case: CrashCase,
}

/// Extract a stable fingerprint from crash report / log text.
pub fn fingerprint_from_text(text: &str, mc_version: &str, loader: &str) -> CrashFingerprint {
    let exception = extract_exception(text);
    let frames = extract_top_frames(text, 5);
    let mod_file = extract_mod_file(text);
    let mixin = extract_mixin(text);
    let mc_major = mc_major(mc_version);
    let loader = loader.to_ascii_lowercase();
    let key = format!(
        "{}|{}|{}|{}|{}",
        normalize_token(&exception),
        frames.first().map(|s| normalize_token(s)).unwrap_or_default(),
        mod_file
            .as_deref()
            .map(normalize_token)
            .unwrap_or_default(),
        mc_major,
        loader
    );
    CrashFingerprint {
        exception,
        frames,
        mod_file,
        mixin,
        mc_major,
        loader,
        key,
    }
}

/// Builtin seed cases (from common Crash Assistant patterns).
pub fn builtin_seed() -> Vec<CrashCase> {
    vec![
        case(
            "builtin-java-version",
            "UnsupportedClassVersionError",
            &["java.lang.UnsupportedClassVersionError"],
            &["wrong java", "class file version"],
            "Minecraft/mod requires a newer Java. Install the Java version required by your MC version (17 for 1.18–1.20.4, 21 for 1.20.5+) and point TuffBox at it.",
            vec![action("config_change", None, "Switch project Java to the required major version", "low")],
        ),
        case(
            "builtin-mixin-apply",
            "Mixin apply failed",
            &["org.spongepowered.asm.mixin.transformer.throwables.MixinTransformerError", "Mixin apply"],
            &["mixin", "incompatible"],
            "A mixin failed to apply — usually a mod version mismatch or two mods patching the same target. Update or temporarily disable the mods named in the mixin error / Mod File line.",
            vec![action("update", None, "Update the mod owning the failing mixin", "medium")],
        ),
        case(
            "builtin-noclassdef",
            "NoClassDefFoundError / ClassNotFoundException",
            &["java.lang.NoClassDefFoundError", "java.lang.ClassNotFoundException"],
            &["missing dependency", "noclass"],
            "A required class is missing — usually a missing library mod (Indium, Architectury, Cloth Config, etc.) or a wrong-loader jar. Install the missing dependency for your loader or remove the broken mod.",
            vec![action("install", None, "Install the missing dependency mod for this loader", "low")],
        ),
        case(
            "builtin-wrong-loader",
            "Wrong mod loader",
            &["ModResolutionException", "Incompatible mods found", "needs mod loader"],
            &["fabric on forge", "neoforge", "wrong loader"],
            "A jar built for a different loader is in mods/. Remove or replace wrong-loader jars so only jars for this project's loader remain.",
            vec![action("remove", None, "Remove wrong-loader jars from mods/", "medium")],
        ),
        case(
            "builtin-out-of-memory",
            "OutOfMemoryError",
            &["java.lang.OutOfMemoryError", "Java heap space", "GC overhead"],
            &["oom", "memory"],
            "The game ran out of heap. Raise allocated RAM in the profile, reduce view distance/shaders, or remove memory-heavy mods.",
            vec![action("config_change", None, "Increase memory allocation for the profile", "low")],
        ),
        case(
            "builtin-opengl",
            "OpenGL / graphics driver",
            &["GLException", "OpenGL", "GLFW error", "iris", "sodium"],
            &["gpu", "shader", "driver"],
            "Graphics/driver or shader pipeline crash. Update GPU drivers, disable shaders temporarily, or update Sodium/Iris/Oculus to versions matching your MC + loader.",
            vec![action("update", None, "Update graphics mods / GPU drivers; try without shaders", "medium")],
        ),
        case(
            "builtin-disk-space",
            "No space left on device",
            &["No space left on device", "ENOSPC"],
            &["disk"],
            "Disk is full. Free space on the drive that holds the instance, then relaunch.",
            vec![action("config_change", None, "Free disk space on the instance drive", "low")],
        ),
        case(
            "builtin-file-locked",
            "File locked by another process",
            &["being used by another process", "AccessDeniedException"],
            &["locked", "antivirus"],
            "A jar or world file is locked. Close other Minecraft/launcher instances and exclude the instance folder from real-time antivirus scanning.",
            vec![action("config_change", None, "Close other MC instances; whitelist instance folder in AV", "low")],
        ),
        case(
            "builtin-kubejs",
            "KubeJS / datapack script error",
            &["kubejs", "ServerEvents", "Failed to load datapack"],
            &["kubejs", "script"],
            "A KubeJS or datapack script failed. Check kubejs/server_scripts and recent recipe/tag edits; fix or temporarily rename the broken script and /reload.",
            vec![action("config_change", None, "Fix or disable the failing KubeJS script", "medium")],
        ),
        case(
            "builtin-create-addon",
            "Create addon mismatch",
            &["create", "ponder", "flywheel"],
            &["create6", "addon"],
            "Create and an add-on are on incompatible versions. Align Create + addons to the same Create major for your MC version.",
            vec![action("update", Some("create".into()), "Match Create and addon versions", "medium")],
        ),
    ]
}

fn case(
    id: &str,
    exception: &str,
    frames: &[&str],
    symptoms: &[&str],
    solution: &str,
    actions: Vec<AiAction>,
) -> CrashCase {
    let frames_owned: Vec<String> = frames.iter().map(|s| (*s).to_string()).collect();
    let key = format!(
        "{}|{}|||",
        normalize_token(exception),
        frames_owned
            .first()
            .map(|s| normalize_token(s))
            .unwrap_or_default()
    );
    CrashCase {
        id: id.into(),
        fingerprint: CrashFingerprint {
            exception: exception.into(),
            frames: frames_owned,
            mod_file: None,
            mixin: None,
            mc_major: String::new(),
            loader: String::new(),
            key,
        },
        symptoms: symptoms.iter().map(|s| (*s).to_string()).collect(),
        suspected_mods: Vec::new(),
        solution: solution.into(),
        actions,
        launcher_actions: Vec::new(),
        notes: None,
        source: "builtin".into(),
        success_count: 1,
        fail_count: 0,
    }
}

fn action(action_type: &str, mod_id: Option<String>, description: &str, risk: &str) -> AiAction {
    AiAction {
        action_type: action_type.into(),
        mod_id,
        description: description.into(),
        risk: risk.into(),
    }
}

pub fn load_all_cases(project_dir: &Path) -> Vec<CrashCase> {
    let mut cases = builtin_seed();
    let path = user_kb_path(project_dir);
    if path.is_file() {
        cases.extend(load_jsonl(&path));
    }
    cases
}

pub fn load_jsonl(path: &Path) -> Vec<CrashCase> {
    let Ok(file) = fs::File::open(path) else {
        return Vec::new();
    };
    let reader = BufReader::new(file);
    let mut out = Vec::new();
    for line in reader.lines().flatten() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(case) = serde_json::from_str::<CrashCase>(line) {
            out.push(case);
        }
    }
    out
}

/// Append or update a user feedback case in the project KB.
pub fn upsert_user_case(project_dir: &Path, case: &CrashCase) -> Result<PathBuf, String> {
    let path = user_kb_path(project_dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut existing = if path.is_file() {
        load_jsonl(&path)
    } else {
        Vec::new()
    };
    if let Some(slot) = existing.iter_mut().find(|c| c.fingerprint.key == case.fingerprint.key) {
        slot.success_count = slot.success_count.saturating_add(case.success_count);
        slot.fail_count = slot.fail_count.saturating_add(case.fail_count);
        if !case.solution.is_empty() {
            slot.solution = case.solution.clone();
        }
        if !case.actions.is_empty() {
            slot.actions = case.actions.clone();
        }
        if !case.launcher_actions.is_empty() {
            slot.launcher_actions = case.launcher_actions.clone();
        }
        if !case.suspected_mods.is_empty() {
            slot.suspected_mods = case.suspected_mods.clone();
        }
        if !case.symptoms.is_empty() {
            slot.symptoms = case.symptoms.clone();
        }
        if case.notes.is_some() {
            slot.notes = case.notes.clone();
        }
        if case.source == "authored" {
            slot.source = "authored".into();
            slot.id = case.id.clone();
        }
    } else {
        existing.push(case.clone());
    }
    let mut file = fs::File::create(&path).map_err(|e| e.to_string())?;
    for c in &existing {
        let line = serde_json::to_string(c).map_err(|e| e.to_string())?;
        writeln!(file, "{line}").map_err(|e| e.to_string())?;
    }
    Ok(path)
}

/// Record a simple helped/wrong vote against matching fingerprint.
pub fn record_feedback(
    project_dir: &Path,
    fingerprint: &CrashFingerprint,
    helped: bool,
    solution: Option<&str>,
    actions: &[AiAction],
    suspected_mods: &[String],
) -> Result<PathBuf, String> {
    let mut case = CrashCase {
        id: format!("user-{}", fingerprint.key.chars().take(24).collect::<String>()),
        fingerprint: fingerprint.clone(),
        symptoms: Vec::new(),
        suspected_mods: suspected_mods.to_vec(),
        solution: solution.unwrap_or("").to_string(),
        actions: actions.to_vec(),
        launcher_actions: Vec::new(),
        notes: None,
        source: "user_feedback".into(),
        success_count: if helped { 1 } else { 0 },
        fail_count: if helped { 0 } else { 1 },
    };
    if case.solution.is_empty() && helped {
        case.solution = "User confirmed this AI explanation helped.".into();
    }
    upsert_user_case(project_dir, &case)
}

/// Rank cases by similarity to the fingerprint + free-text haystack.
pub fn search_similar(
    cases: &[CrashCase],
    fp: &CrashFingerprint,
    haystack: &str,
    k: usize,
) -> Vec<SimilarCaseHit> {
    let hay = haystack.to_ascii_lowercase();
    let mut scored: Vec<(f64, &CrashCase)> = cases
        .iter()
        .filter(|c| c.fail_count <= c.success_count.saturating_add(2))
        .map(|c| (score_case(c, fp, &hay), c))
        .filter(|(s, _)| *s > 0.15)
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(k);
    scored
        .into_iter()
        .map(|(score, c)| {
            let actions = if !c.launcher_actions.is_empty() {
                crate::action_plan::plan_to_legacy_ai_actions(&crate::action_plan::ActionPlan {
                    schema_version: crate::action_plan::ACTION_PLAN_SCHEMA_VERSION,
                    human_explanation: c.solution.clone(),
                    confidence: score,
                    suspected_mods: c.suspected_mods.clone(),
                    needs_user_review: true,
                    source: Some("kb".into()),
                    matched_case_ids: vec![c.id.clone()],
                    actions: c.launcher_actions.clone(),
                    additional_context: None,
                })
            } else {
                c.actions.clone()
            };
            SimilarCaseHit {
                id: c.id.clone(),
                score,
                solution: c.solution.clone(),
                suspected_mods: c.suspected_mods.clone(),
                actions,
                fingerprint_key: c.fingerprint.key.clone(),
                source: c.source.clone(),
            }
        })
        .collect()
}

/// Save an authored crash→resolution case for the pack author.
/// Writes into project `cases.jsonl` and a public export JSON (notes stripped).
pub fn save_authored_case(
    project_dir: &Path,
    input: AuthorCaseInput,
) -> Result<AuthorCaseSaveResult, String> {
    let solution = input.solution.trim().to_string();
    if solution.is_empty() {
        return Err("solution text is required".into());
    }
    if input.fingerprint.key.trim().is_empty() {
        return Err("fingerprint.key is required".into());
    }

    let mut launcher_actions = input.launcher_actions;
    if launcher_actions.is_empty() && !input.actions.is_empty() {
        launcher_actions = input
            .actions
            .iter()
            .map(|a| LauncherAction {
                op: match a.action_type.to_ascii_lowercase().as_str() {
                    "update" | "update_mod" => "update_mod".into(),
                    "remove" | "remove_mod" => "remove_mod".into(),
                    "install" | "install_mod" => "install_mod".into(),
                    "disable" | "disable_mod" => "disable_mod".into(),
                    "config_change" | "edit_config" => "edit_config".into(),
                    "reinstall" | "reinstall_mod" => "reinstall_mod".into(),
                    other => other.to_string(),
                },
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
    }

    let legacy_actions = if input.actions.is_empty() && !launcher_actions.is_empty() {
        crate::action_plan::plan_to_legacy_ai_actions(&crate::action_plan::ActionPlan {
            schema_version: crate::action_plan::ACTION_PLAN_SCHEMA_VERSION,
            human_explanation: solution.clone(),
            confidence: 1.0,
            suspected_mods: input.suspected_mods.clone(),
            needs_user_review: false,
            source: Some("kb".into()),
            matched_case_ids: vec![],
            actions: launcher_actions.clone(),
            additional_context: None,
        })
    } else {
        input.actions
    };

    let id = input
        .id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            let slug: String = input
                .fingerprint
                .exception
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
                .take(32)
                .collect::<String>()
                .to_ascii_lowercase();
            let key_bit: String = input.fingerprint.key.chars().take(16).collect();
            format!(
                "authored-{}-{}",
                if slug.is_empty() { "case".into() } else { slug },
                key_bit
            )
        });

    let case = CrashCase {
        id: id.clone(),
        fingerprint: input.fingerprint,
        symptoms: input.symptoms,
        suspected_mods: input.suspected_mods,
        solution,
        actions: legacy_actions,
        launcher_actions,
        notes: input.notes.filter(|s| !s.trim().is_empty()),
        source: "authored".into(),
        success_count: 1,
        fail_count: 0,
    };

    let kb_path = upsert_user_case(project_dir, &case)?;

    let export_dir = author_export_dir(project_dir);
    fs::create_dir_all(&export_dir).map_err(|e| e.to_string())?;
    let export_path = export_dir.join(format!("{id}.json"));
    let public = public_case_for_export(&case);
    fs::write(
        &export_path,
        serde_json::to_vec_pretty(&public).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    Ok(AuthorCaseSaveResult {
        case_id: id,
        kb_path: kb_path.to_string_lossy().to_string(),
        export_path: export_path.to_string_lossy().to_string(),
        case,
    })
}

/// Strip author-only fields for remote/public export.
pub fn public_case_for_export(case: &CrashCase) -> serde_json::Value {
    serde_json::json!({
        "id": case.id,
        "fingerprint": case.fingerprint,
        "symptoms": case.symptoms,
        "suspectedMods": case.suspected_mods,
        "solution": case.solution,
        "actions": case.launcher_actions,
        "source": "authored",
        "successCount": case.success_count,
        "failCount": case.fail_count,
    })
}

/// List authored cases in the project KB (newest first by file order reverse).
pub fn list_authored_cases(project_dir: &Path) -> Vec<CrashCase> {
    load_all_cases(project_dir)
        .into_iter()
        .filter(|c| c.source == "authored")
        .rev()
        .collect()
}

#[cfg(test)]
mod author_tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn saves_authored_case_and_export() {
        let dir = tempdir().unwrap();
        let fp = CrashFingerprint {
            exception: "NoClassDefFoundError".into(),
            frames: vec!["com.example.Foo".into()],
            mod_file: Some("foo.jar".into()),
            mixin: None,
            mc_major: "1.20".into(),
            loader: "fabric".into(),
            key: "noclass|foo|foo.jar|1.20|fabric".into(),
        };
        let result = save_authored_case(
            dir.path(),
            AuthorCaseInput {
                id: Some("test-indium".into()),
                fingerprint: fp,
                solution: "Install Indium for Sodium".into(),
                symptoms: vec!["NoClassDefFoundError".into()],
                suspected_mods: vec!["sodium".into()],
                launcher_actions: vec![LauncherAction {
                    op: "install_mod".into(),
                    mod_id: Some("indium".into()),
                    provider: Some("modrinth".into()),
                    project_id: None,
                    version: None,
                    path: None,
                    patch_type: None,
                    patch: None,
                    reason: Some("Missing Indium".into()),
                    risk: "low".into(),
                }],
                actions: vec![],
                notes: Some("internal note".into()),
            },
        )
        .unwrap();
        assert_eq!(result.case_id, "test-indium");
        assert!(Path::new(&result.kb_path).is_file());
        assert!(Path::new(&result.export_path).is_file());
        let export: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&result.export_path).unwrap()).unwrap();
        assert!(export.get("notes").is_none());
        assert_eq!(export["actions"][0]["op"], "install_mod");
        let listed = list_authored_cases(dir.path());
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].notes.as_deref(), Some("internal note"));
    }
}

fn score_case(case: &CrashCase, fp: &CrashFingerprint, hay: &str) -> f64 {
    let mut score = 0.0;
    if !fp.key.is_empty() && fp.key == case.fingerprint.key {
        score += 1.0;
    }
    let ex = normalize_token(&fp.exception);
    let cex = normalize_token(&case.fingerprint.exception);
    if !ex.is_empty() && !cex.is_empty() {
        if ex.contains(&cex) || cex.contains(&ex) {
            score += 0.55;
        } else if jaccard_tokens(&ex, &cex) > 0.35 {
            score += 0.35;
        }
    }
    for frame in &fp.frames {
        let nf = normalize_token(frame);
        for cf in &case.fingerprint.frames {
            let ncf = normalize_token(cf);
            if !nf.is_empty() && (nf.contains(&ncf) || ncf.contains(&nf)) {
                score += 0.2;
                break;
            }
        }
    }
    if let (Some(a), Some(b)) = (&fp.mod_file, &case.fingerprint.mod_file) {
        if normalize_token(a) == normalize_token(b) {
            score += 0.25;
        }
    }
    for symptom in &case.symptoms {
        if hay.contains(&symptom.to_ascii_lowercase()) {
            score += 0.12;
        }
    }
    // Prefer proven cases slightly.
    score += (case.success_count as f64) * 0.02;
    score -= (case.fail_count as f64) * 0.03;
    if !case.fingerprint.loader.is_empty()
        && !fp.loader.is_empty()
        && case.fingerprint.loader == fp.loader
    {
        score += 0.05;
    }
    score
}

fn jaccard_tokens(a: &str, b: &str) -> f64 {
    let ta: HashSet<&str> = a.split(|c: char| !c.is_ascii_alphanumeric()).filter(|t| t.len() > 2).collect();
    let tb: HashSet<&str> = b.split(|c: char| !c.is_ascii_alphanumeric()).filter(|t| t.len() > 2).collect();
    if ta.is_empty() || tb.is_empty() {
        return 0.0;
    }
    let inter = ta.intersection(&tb).count() as f64;
    let union = ta.union(&tb).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        inter / union
    }
}

fn normalize_token(s: &str) -> String {
    s.to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '/' { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

fn mc_major(version: &str) -> String {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() >= 2 {
        format!("{}.{}", parts[0], parts[1])
    } else {
        version.to_string()
    }
}

fn extract_exception(text: &str) -> String {
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("Description:") {
            return t.trim_start_matches("Description:").trim().to_string();
        }
        for prefix in [
            "java.lang.",
            "java.io.",
            "org.spongepowered.",
            "net.minecraft.",
            "cpw.mods.",
            "net.fabricmc.",
            "net.neoforged.",
        ] {
            if t.starts_with(prefix) && (t.contains("Exception") || t.contains("Error")) {
                // Prefer the FQCN token (strip message after ':' / whitespace).
                let token = t.split([':', ' ', '\t']).next().unwrap_or(t).trim();
                return token.to_string();
            }
        }
        if let Some(rest) = t.strip_prefix("Caused by:") {
            let rest = rest.trim();
            let token = rest.split([':', ' ', '\t']).next().unwrap_or(rest).trim();
            if token.contains('.') {
                return token.to_string();
            }
        }
    }
    String::new()
}

fn extract_top_frames(text: &str, n: usize) -> Vec<String> {
    let mut frames = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("at ") {
            let frame = t.trim_start_matches("at ").trim();
            // Drop line numbers: Foo.bar(Foo.java:12) → Foo.bar
            let cleaned = frame
                .split('(')
                .next()
                .unwrap_or(frame)
                .trim()
                .to_string();
            if !cleaned.is_empty() {
                frames.push(cleaned);
            }
            if frames.len() >= n {
                break;
            }
        }
    }
    frames
}

fn extract_mod_file(text: &str) -> Option<String> {
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("Mod File:") {
            let v = rest.trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
        if let Some(rest) = t.strip_prefix("Mod:") {
            let v = rest.trim();
            if !v.is_empty() && v.contains('.') {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn extract_mixin(text: &str) -> Option<String> {
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("mixin") && (lower.contains("apply") || lower.contains("failed") || lower.contains("@mixin")) {
            return Some(line.trim().chars().take(200).collect());
        }
    }
    None
}

/// Focused excerpt around Description / Exception / Caused by / Head.
pub fn smart_excerpt(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        return text.to_string();
    }
    let lines: Vec<&str> = text.lines().collect();
    let mut best = 0usize;
    for (i, line) in lines.iter().enumerate() {
        let l = line.to_ascii_lowercase();
        if l.contains("description:")
            || l.contains("caused by:")
            || l.contains("-- head --")
            || l.contains("exception")
            || l.contains("mixin apply")
            || l.contains("mod file:")
        {
            best = i;
            break;
        }
    }
    let start = best.saturating_sub(15);
    let end = (best + 80).min(lines.len());
    let chunk = lines[start..end].join("\n");
    if chunk.len() <= max_len {
        return chunk;
    }
    format!("{}... (truncated)", &chunk[..max_len])
}

/// Append a raw JSONL line (debug / import helper).
pub fn append_case_line(project_dir: &Path, case: &CrashCase) -> Result<(), String> {
    let path = user_kb_path(project_dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    let line = serde_json::to_string(case).map_err(|e| e.to_string())?;
    writeln!(file, "{line}").map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints_exception() {
        let text = "Description: Unexpected error\njava.lang.NoClassDefFoundError: com/example/Foo\n\tat com.example.Bar.run(Bar.java:10)\nMod File: sodium.jar\n";
        let fp = fingerprint_from_text(text, "1.20.1", "fabric");
        assert!(fp.exception.to_lowercase().contains("noclassdef") || fp.exception.contains("Unexpected"));
        assert!(!fp.frames.is_empty());
        assert_eq!(fp.mc_major, "1.20");
    }

    #[test]
    fn finds_builtin_similar() {
        let cases = builtin_seed();
        let fp = fingerprint_from_text(
            "java.lang.OutOfMemoryError: Java heap space\nat net.minecraft.client.main.Main.main(Main.java:1)\n",
            "1.20.1",
            "fabric",
        );
        let hits = search_similar(&cases, &fp, "java heap space oom", 3);
        assert!(!hits.is_empty());
        assert!(hits[0].solution.to_lowercase().contains("ram") || hits[0].solution.to_lowercase().contains("memory"));
    }
}
