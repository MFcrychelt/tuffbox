//! Crash Assistant — full analysis engine (30+ crash patterns).
//!
//! Adapted from KostromDan's Crash Assistant mod. Every analysis module
//! from the mod is implemented as a launcher-side function that runs on
//! crash logs before the user even starts a report. Each module produces
//! structured findings with severity, title, description, auto-fix
//! instructions, and references.
//!
//! Also includes the Package/Class Finder and Jdeps analysis tools from
//! Crash Assistant's GUI.

use crate::launch_error::{LaunchErrorInfo, LaunchErrorKind};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashAnalysisFinding {
    pub severity: String,
    pub code: String,
    pub title: String,
    pub description: String,
    pub auto_fix: Option<String>,
    pub references: Vec<String>,
    /// Machine-actionable fixes the Diagnose UI can apply one-by-one.
    #[serde(default)]
    pub fixes: Vec<crate::crash::FixAction>,
    /// Matched log / crash-report excerpt that triggered this finding.
    #[serde(default)]
    pub evidence: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashAnalysisReport {
    pub findings: Vec<CrashAnalysisFinding>,
    pub support_message_discord: String,
    pub support_message_github: String,
    pub mods_added: Vec<String>,
    pub mods_removed: Vec<String>,
    pub suspected_mods: Vec<String>,
    pub mcreator_mods: Vec<String>,
    pub class_finder_results: Vec<ClassMatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassMatch {
    pub class_name: String,
    pub mod_id: String,
    pub mod_name: String,
    #[serde(default)]
    pub file_name: Option<String>,
}

pub struct AnalysisCtx {
    pub crash_content: Vec<String>,
    pub latest_log: String,
    pub launcher_log: String,
    pub installed_mods: Vec<String>,
    pub previous_mods: Vec<String>,
    pub java_version: String,
    pub java_vendor: String,
    pub os_name: String,
    pub mc_version: String,
    pub loader: String,
    pub loader_version: String,
    pub cpu_name: String,
    pub gpu_names: Vec<String>,
    pub total_ram_mb: u64,
    pub is_offline: bool,
    pub win_events: Vec<String>,
}

// ═══════════════════════════════════════════════════════════════════

pub fn run_full_analysis(ctx: &AnalysisCtx) -> CrashAnalysisReport {
    let mut findings = Vec::new();
    let combined: String =
        ctx.crash_content.join("\n") + "\n" + &ctx.latest_log + "\n" + &ctx.launcher_log;

    findings.extend(check_java_version(ctx, &combined));
    findings.extend(check_mixins(ctx, &combined));
    findings.extend(check_missing_mods(ctx, &combined));
    findings.extend(check_intel_cpu(ctx));
    findings.extend(check_integrated_gpu(ctx));
    findings.extend(check_offline(ctx));
    findings.extend(check_corrupted(ctx, &combined));
    findings.extend(check_module_resolution(ctx, &combined));
    findings.extend(check_connector_incompat(ctx, &combined));
    findings.extend(check_too_many_ids(&combined));
    findings.extend(check_create6_addons(ctx, &combined));
    findings.extend(check_epic_fight_addons(ctx, &combined));
    findings.extend(check_neoforge_version(ctx));
    findings.extend(check_used_by_another_process(&combined));
    findings.extend(check_groovy_ipv6(&combined));
    findings.extend(check_disk_space(&combined));
    findings.extend(check_kubejs_datapack(&combined));
    findings.extend(check_language_provider_mismatch(&combined));
    findings.extend(check_modernfix_watchdog(&combined));
    findings.extend(check_feature_order_cycle(&combined));
    findings.extend(check_medieval_origins(ctx, &combined));
    findings.extend(check_geckolib_oculus(ctx));
    findings.extend(check_intel_driver(&combined));
    findings.extend(check_macos_shader_driver(ctx, &combined));
    findings.extend(check_jvm_dll_error(&combined));
    findings.extend(check_corrupted_mod_jar(&combined));
    findings.extend(check_watermedia_vlc(&combined));
    findings.extend(check_irlandacore_backdoor(ctx));
    findings.extend(check_class_metadata_not_found(&combined));
    findings.extend(check_mcreator_mods(&ctx.installed_mods));
    findings.extend(check_conflict_log_phrases(ctx, &combined));

    // Deduplicate by code (keep first / highest-severity order).
    let mut seen = std::collections::HashSet::new();
    findings.retain(|f| seen.insert(f.code.clone()));

    let suspected = extract_suspected(ctx, &combined);
    let (added, removed) = compute_mod_diff(ctx);
    let mcreator = find_mcreator_mods(&ctx.installed_mods);
    let class_matches = find_classes_in_crashes(ctx, &combined);

    CrashAnalysisReport {
        support_message_discord: build_message(ctx, &findings, "discord"),
        support_message_github: build_message(ctx, &findings, "github"),
        findings,
        mods_added: added,
        mods_removed: removed,
        suspected_mods: suspected,
        mcreator_mods: mcreator,
        class_finder_results: class_matches,
    }
}

// ═══════════════════════════════════════════════════════════════════
// Analysis modules

fn f(
    severity: &str,
    code: &str,
    title: &str,
    description: &str,
    auto_fix: Option<&str>,
    refs: &[&str],
) -> CrashAnalysisFinding {
    fx(severity, code, title, description, auto_fix, refs, vec![], None)
}

fn fx(
    severity: &str,
    code: &str,
    title: &str,
    description: &str,
    auto_fix: Option<&str>,
    refs: &[&str],
    fixes: Vec<crate::crash::FixAction>,
    evidence: Option<String>,
) -> CrashAnalysisFinding {
    CrashAnalysisFinding {
        severity: severity.into(),
        code: code.into(),
        title: title.into(),
        description: description.into(),
        auto_fix: auto_fix.map(|s| s.into()),
        references: refs.iter().map(|s| s.to_string()).collect(),
        fixes,
        evidence,
    }
}

fn fix_action(kind: &str, label: &str, mod_id: Option<&str>) -> crate::crash::FixAction {
    crate::crash::FixAction {
        kind: kind.into(),
        label: label.into(),
        mod_id: mod_id.map(|s| s.into()),
    }
}

fn match_mods_in_text(text: &str, installed: &[String]) -> Vec<String> {
    let lower = text.to_lowercase();
    let mut hits = Vec::new();
    for m in installed {
        let id = m.to_lowercase();
        // Short / generic ids match almost every Fabric "Loading mods:" line.
        if id.len() < 3 || matches!(id.as_str(), "api" | "lib" | "mod" | "core" | "common") {
            continue;
        }
        let needle = id.replace('_', "-");
        let needle_us = id.replace('-', "_");
        if contains_mod_token(&lower, &id)
            || contains_mod_token(&lower, &needle)
            || contains_mod_token(&lower, &needle_us)
        {
            hits.push(m.clone());
        }
    }
    hits.sort();
    hits.dedup();
    hits
}

/// True when `needle` appears as a whole mod-id token (not a substring of another word).
fn contains_mod_token(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let mut start = 0;
    while let Some(rel) = haystack[start..].find(needle) {
        let abs = start + rel;
        let before_ok = abs == 0
            || !haystack.as_bytes()[abs - 1].is_ascii_alphanumeric();
        let end = abs + needle.len();
        let after_ok = end >= haystack.len()
            || !haystack.as_bytes()[end].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
        start = abs + 1;
    }
    false
}

fn first_evidence_line<'a>(combined: &'a str, needles: &[&str]) -> Option<&'a str> {
    for line in combined.lines() {
        let trimmed = line.trim();
        if trimmed.len() < 8 {
            continue;
        }
        // Skip Fabric/Quilt "Loading X mods: a, b, c, …" inventory dumps.
        if looks_like_mod_inventory_line(trimmed) {
            continue;
        }
        let l = trimmed.to_lowercase();
        if needles.iter().any(|n| l.contains(&n.to_lowercase())) {
            return Some(trimmed);
        }
    }
    None
}

fn looks_like_mod_inventory_line(line: &str) -> bool {
    let lower = line.to_lowercase();
    if lower.contains("loading") && lower.contains("mods:") {
        return true;
    }
    // Long comma-separated mod-id lists with almost no spaces after commas.
    let commas = line.matches(',').count();
    commas >= 12 && line.len() > 180
}

fn truncate_evidence(line: &str) -> String {
    const MAX: usize = 280;
    let t = line.trim();
    if t.len() <= MAX {
        return t.to_string();
    }
    format!("{}…", &t[..MAX])
}

fn extract_required_mod_ids(combined: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in combined.lines() {
        let lower = line.to_lowercase();
        if !(lower.contains("requires")
            || lower.contains("missing")
            || lower.contains("dependency"))
        {
            continue;
        }
        for part in line.split(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_') {
            let p = part.trim().to_lowercase();
            if p.len() >= 3
                && p.len() <= 48
                && !matches!(
                    p.as_str(),
                    "requires"
                        | "missing"
                        | "mandatory"
                        | "dependency"
                        | "dependencies"
                        | "version"
                        | "minecraft"
                        | "fabricloader"
                        | "forge"
                        | "neoforge"
                        | "quilt"
                        | "which"
                        | "mod"
                        | "any"
                        | "of"
                        | "but"
                        | "is"
                        | "the"
                        | "and"
                        | "for"
                        | "from"
                        | "with"
                        | "this"
                        | "that"
                        | "error"
                        | "exception"
                        | "failed"
                        | "loading"
                )
            {
                if lower.contains(&format!("'{p}'"))
                    || lower.contains(&format!("\"{p}\""))
                    || lower.contains(&format!("`{p}`"))
                {
                    if !out.iter().any(|x: &String| x == &p) {
                        out.push(p);
                    }
                }
            }
        }
    }
    out.into_iter().take(12).collect()
}

fn check_java_version(ctx: &AnalysisCtx, combined: &str) -> Vec<CrashAnalysisFinding> {
    let mut out = Vec::new();
    if let Some(ver) = extract_major(combined) {
        let needed = m2j(ver);
        out.push(f(
            "error",
            "UNSUPPORTED_CLASS_VERSION",
            &format!(
                "Mod needs Java {needed} (running Java {})",
                ctx.java_version
            ),
            &format!("Class file version {} → Java {}+ required.", ver, needed),
            Some(&format!("Install Java {}+ in Project Settings.", needed)),
            &["https://adoptium.net/"],
        ));
    }
    if combined.contains("UnsupportedClassVersionError") {
        out.push(f(
            "error",
            "JAVA_VERSION_MISMATCH",
            "Java version mismatch",
            "UnsupportedClassVersionError in log.",
            Some("Install Java 21 LTS."),
            &["https://adoptium.net/"],
        ));
    }
    out
}

fn extract_major(text: &str) -> Option<u32> {
    for l in text.lines() {
        for w in l.split_whitespace() {
            if let Ok(n) = w.parse::<u32>() {
                if (45..=69).contains(&n) {
                    return Some(n);
                }
            }
        }
    }
    None
}
fn m2j(m: u32) -> String {
    match m {
        52 => "8".into(),
        55 => "11".into(),
        60 => "16".into(),
        61 => "17".into(),
        62 => "18".into(),
        63 => "19".into(),
        64 => "20".into(),
        65 => "21".into(),
        66 => "22".into(),
        67 => "23".into(),
        68 => "24".into(),
        _ => format!("?({m})"),
    }
}

fn check_mixins(ctx: &AnalysisCtx, combined: &str) -> Vec<CrashAnalysisFinding> {
    if combined.contains("Mixin")
        && (combined.contains("failed")
            || combined.contains("Error")
            || combined.contains("Exception"))
    {
        // Only scan lines that actually mention mixin failure — not the
        // Fabric "Loading mods:" inventory that substring-matches short ids.
        let mixin_lines: String = combined
            .lines()
            .filter(|l| {
                let lower = l.to_lowercase();
                (lower.contains("mixin") || lower.contains("@inject") || lower.contains("@redirect"))
                    && !looks_like_mod_inventory_line(l)
            })
            .collect::<Vec<_>>()
            .join("\n");
        let search = if mixin_lines.is_empty() {
            combined
        } else {
            mixin_lines.as_str()
        };

        let mut suspect = String::new();
        for m in &ctx.installed_mods {
            if contains_mod_token(&search.to_lowercase(), &m.to_lowercase()) {
                suspect = m.clone();
                break;
            }
        }
        let mut refs = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for m in match_mods_in_text(search, &ctx.installed_mods) {
            if seen.insert(m.clone()) {
                refs.push(m);
            }
            if refs.len() >= 5 {
                break;
            }
        }
        let title_s = format!(
            "Mixin failure: {}",
            if suspect.is_empty() {
                "unknown mod"
            } else {
                &suspect
            }
        );
        let desc_s = format!(
            "Mixin injection failed. Affected mods: {}",
            refs.iter().take(5).cloned().collect::<Vec<_>>().join(", ")
        );
        let fix_s = if suspect.is_empty() {
            None
        } else {
            Some(format!(
                "Update/remove '{}'. Check {} {} compatibility.",
                suspect, ctx.loader, ctx.mc_version
            ))
        };
        let targets: Vec<String> = if refs.is_empty() {
            if suspect.is_empty() {
                Vec::new()
            } else {
                vec![suspect.clone()]
            }
        } else {
            refs.iter().take(3).cloned().collect()
        };
        let fixes: Vec<_> = targets
            .iter()
            .flat_map(|m| {
                vec![
                    fix_action("updateMod", &format!("Update `{m}`"), Some(m)),
                    fix_action("disableMod", &format!("Disable `{m}`"), Some(m)),
                ]
            })
            .collect();
        let evidence = first_evidence_line(
            search,
            &["Mixin apply failed", "Mixin", "@Inject", "mixin"],
        )
        .map(truncate_evidence);
        vec![fx(
            "error",
            "MIXIN_APPLY_FAILED",
            &title_s,
            &desc_s,
            fix_s.as_deref(),
            &[],
            fixes,
            evidence,
        )]
    } else {
        vec![]
    }
}

fn check_missing_mods(ctx: &AnalysisCtx, combined: &str) -> Vec<CrashAnalysisFinding> {
    let mut out = Vec::new();
    let has = |s: &str| ctx.installed_mods.contains(&s.to_string());
    if has("sodium") && !has("indium") && ctx.loader == "fabric" {
        out.push(fx(
            "error",
            "MISSING_INDIUM",
            "Indium is missing",
            "Sodium on Fabric needs Indium for Fabric Renderer API.",
            Some("Install Indium from Modrinth."),
            &["https://modrinth.com/mod/indium"],
            vec![fix_action(
                "installDependency",
                "Install Indium",
                Some("indium"),
            )],
            None,
        ));
    }
    if has("oculus")
        && !has("embeddium")
        && !has("rubidium")
        && (ctx.loader == "forge" || ctx.loader == "neoforge")
    {
        out.push(f(
            "error",
            "MISSING_EMBEDDIUM",
            "Oculus needs Embeddium",
            "Oculus requires Embeddium/Rubidium as rendering backend.",
            Some("Install Embeddium from Modrinth."),
            &["https://modrinth.com/mod/embeddium"],
        ));
    }
    if combined.contains("Connector")
        && (combined.contains("MissingConnectorDependency")
            || combined.contains("Connector dependency"))
    {
        out.push(f(
            "warning",
            "CONNECTOR_MISSING_DEP",
            "Connector missing dependency",
            "Sinytra Connector may need additional Fabric bridging mods.",
            Some("Check Connector mod page for required dependencies."),
            &["https://modrinth.com/mod/connector"],
        ));
    }
    out
}

fn check_intel_cpu(ctx: &AnalysisCtx) -> Vec<CrashAnalysisFinding> {
    let c = ctx.cpu_name.to_lowercase();
    if c.contains("intel")
        && (c.contains("139")
            || c.contains("137")
            || c.contains("149")
            || c.contains("147")
            || (c.contains("13") && c.contains("th"))
            || (c.contains("14") && c.contains("th")))
    {
        vec![f(
            "warning",
            "INTEL_13_14_GEN_CPU",
            "Intel 13/14 Gen CPU instability",
            "These CPUs have a known stability bug — update BIOS for microcode 0x129+.",
            Some("Update motherboard BIOS to latest version."),
            &["https://community.intel.com/"],
        )]
    } else {
        vec![]
    }
}

fn check_integrated_gpu(ctx: &AnalysisCtx) -> Vec<CrashAnalysisFinding> {
    let has_ded = ctx.gpu_names.iter().any(|g| {
        g.to_lowercase().contains("nvidia")
            || g.to_lowercase().contains("amd")
            || g.to_lowercase().contains("radeon")
    });
    let has_int = ctx
        .gpu_names
        .iter()
        .any(|g| g.to_lowercase().contains("intel"));
    if has_ded && has_int && ctx.os_name.to_lowercase().contains("windows") {
        vec![f(
            "warning",
            "INTEGRATED_GPU",
            "Minecraft may use integrated GPU",
            "Dedicated GPU available but game may run on Intel integrated.",
            Some("Windows Settings → Display → Graphics → Browse → javaw.exe → High Performance."),
            &[],
        )]
    } else {
        vec![]
    }
}

fn check_offline(ctx: &AnalysisCtx) -> Vec<CrashAnalysisFinding> {
    if ctx.is_offline {
        vec![f(
            "info",
            "OFFLINE_MODE",
            "Running offline",
            "Offline mode detected — some support channels don't assist cracked launchers.",
            None,
            &[],
        )]
    } else {
        vec![]
    }
}

fn check_corrupted(_ctx: &AnalysisCtx, combined: &str) -> Vec<CrashAnalysisFinding> {
    if combined.contains("Invalid signature")
        || (combined.contains("SHA1") && combined.contains("mismatch"))
        || combined.contains("corrupted") && combined.contains("jar")
    {
        vec![f(
            "error",
            "CORRUPTED_INSTALL",
            "Installation may be corrupted",
            "File verification failed — some game files are damaged.",
            Some("Use 'Repair Profile' in Dashboard menu to re-download."),
            &[],
        )]
    } else {
        vec![]
    }
}

fn check_module_resolution(ctx: &AnalysisCtx, combined: &str) -> Vec<CrashAnalysisFinding> {
    for line in combined.lines() {
        let lo = line.to_lowercase();
        if lo.contains("noclassdeffounderror")
            || lo.contains("classnotfoundexception")
            || lo.contains("module") && lo.contains("not found")
        {
            let class = line
                .split(": ")
                .nth(1)
                .unwrap_or("?")
                .split_whitespace()
                .next()
                .unwrap_or("?");
            let cn = if class.len() > 200 {
                "unknown class"
            } else {
                class
            };
            // Try to find which mod provides it
            let mut suggested = String::new();
            for m in &ctx.installed_mods {
                let ml = m.replace('-', "/");
                if cn.to_lowercase().contains(&ml)
                    || ml.contains(&cn.to_lowercase().replace('.', "/"))
                {
                    suggested = m.clone();
                    break;
                }
            }
            let fix = if !suggested.is_empty() {
                format!("Check if '{}' is installed. {}", suggested, cn)
            } else {
                format!("Search '{}' on Modrinth.", cn)
            };
            return vec![f(
                "error",
                "NOCLASSDEFFOUND",
                &format!("Missing: {cn}"),
                "A class is missing — likely a missing or wrong-version dependency.",
                Some(&fix),
                &["https://modrinth.com"],
            )];
        }
    }
    vec![]
}

fn check_connector_incompat(ctx: &AnalysisCtx, combined: &str) -> Vec<CrashAnalysisFinding> {
    if combined.contains("Connector") && combined.contains("incompatible") {
        let bad: Vec<_> = ["sodium", "iris", "indium", "lithium", "phosphor"]
            .iter()
            .filter(|m| ctx.installed_mods.contains(&m.to_string()))
            .cloned()
            .collect();
        if !bad.is_empty() {
            vec![f(
                "error",
                "CONNECTOR_INCOMPAT",
                "Connector incompatible Fabric mods",
                &format!(
                    "These Fabric mods don't work with Sinytra Connector: {}",
                    bad.join(", ")
                ),
                Some(&format!(
                    "Remove: {}. Forge alternatives: Embeddium (Sodium), Oculus (Iris).",
                    bad.join(", ")
                )),
                &[],
            )]
        } else {
            vec![]
        }
    } else {
        vec![]
    }
}

fn check_too_many_ids(combined: &str) -> Vec<CrashAnalysisFinding> {
    if combined.contains("maximum id range")
        || combined.contains("exceeded id limit")
        || combined.contains("maximum block ID")
    {
        vec![f(
            "error",
            "TOO_MANY_IDS",
            "Exceeded block/item ID limit",
            "Too many mods for legacy MC (1.12.2-).",
            Some("Install NotEnoughIDs, JEID, or remove mods."),
            &["https://www.curseforge.com/minecraft/mc-mods/notenoughids"],
        )]
    } else {
        vec![]
    }
}

fn check_create6_addons(ctx: &AnalysisCtx, combined: &str) -> Vec<CrashAnalysisFinding> {
    let has_create = ctx.installed_mods.iter().any(|m| m.contains("create"));
    if !has_create {
        return vec![];
    }
    if combined.contains("Create")
        && (combined.contains("NoSuchMethod")
            || combined.contains("NoSuchField")
            || combined.contains("IncompatibleClassChangeError"))
    {
        let addons: Vec<_> = ctx
            .installed_mods
            .iter()
            .filter(|m| m.contains("create") && *m != "create")
            .cloned()
            .collect();
        let fix = if !addons.is_empty() {
            format!(
                "Suspected addons: {}. Use 'Check updates' in Mods page.",
                addons.join(", ")
            )
        } else {
            "Update Create and all addons to compatible versions.".into()
        };
        vec![f(
            "error",
            "CREATE6_ADDONS_INCOMPAT",
            "Create addon version mismatch",
            "Create version incompatible with installed addons.",
            Some(&fix),
            &[],
        )]
    } else {
        vec![]
    }
}

fn check_epic_fight_addons(ctx: &AnalysisCtx, combined: &str) -> Vec<CrashAnalysisFinding> {
    let has_ef = ctx
        .installed_mods
        .iter()
        .any(|m| m.contains("epicfight") || m.contains("epic_fight"));
    if !has_ef {
        return vec![];
    }
    if combined.contains("EpicFight")
        || combined.contains("epicfight")
            && (combined.contains("NoSuchMethod") || combined.contains("NoSuchField"))
    {
        let addons: Vec<_> = ctx
            .installed_mods
            .iter()
            .filter(|m| m.contains("epic"))
            .cloned()
            .collect();
        let fix = format!(
            "Suspected Epic Fight addons: {}. Check for update compatibility.",
            addons.join(", ")
        );
        vec![f(
            "error",
            "EPICFIGHT_ADDONS_INCOMPAT",
            "Epic Fight addon mismatch",
            "Epic Fight version incompatible with installed addons.",
            Some(&fix),
            &[],
        )]
    } else {
        vec![]
    }
}

fn check_neoforge_version(ctx: &AnalysisCtx) -> Vec<CrashAnalysisFinding> {
    if ctx.loader == "neoforge" && ctx.mc_version == "1.20.1" {
        vec![f(
            "warning",
            "NEOFORGE_1_20_1_ABANDONED",
            "NeoForge on 1.20.1 is abandoned",
            "NeoForge team recommends switching to Forge on 1.20.1.",
            Some("Consider migrating to Forge for 1.20.1."),
            &[],
        )]
    } else {
        vec![]
    }
}

fn check_used_by_another_process(combined: &str) -> Vec<CrashAnalysisFinding> {
    if combined.contains("used by another process")
        || combined.contains("file is locked")
        || combined.contains("FileLock")
    {
        vec![f(
            "warning",
            "USED_BY_ANOTHER_PROCESS",
            "File locked by another process",
            "A config or world file is locked by another Minecraft instance or program.",
            Some("Close other Minecraft instances, restart launcher."),
            &[],
        )]
    } else {
        vec![]
    }
}

fn check_groovy_ipv6(combined: &str) -> Vec<CrashAnalysisFinding> {
    if combined.contains("GroovyModLoader")
        || (combined.contains("GML") && combined.contains("Failed"))
    {
        vec![f(
            "warning",
            "GML_IPV6",
            "GroovyModLoader network issue",
            "GML failed to download mappings — possible IPv6 issue.",
            Some("Disable IPv6 in network settings or use a VPN."),
            &[],
        )]
    } else {
        vec![]
    }
}

fn check_disk_space(combined: &str) -> Vec<CrashAnalysisFinding> {
    if combined.contains("No space left on device")
        || combined.contains("Disk full")
        || combined.contains("out of disk")
    {
        vec![f(
            "error",
            "DISK_SPACE_ENDED",
            "Out of disk space",
            "Game cannot save — disk is full.",
            Some("Free up disk space and restart."),
            &[],
        )]
    } else {
        vec![]
    }
}

fn check_kubejs_datapack(combined: &str) -> Vec<CrashAnalysisFinding> {
    if (combined.contains("kubejs") || combined.contains("KubeJS"))
        && (combined.contains("datapack") || combined.contains("data pack"))
        && combined.contains("error")
    {
        vec![f("error","KUBEJS_DATAPACK","KubeJS datapack loading error","A KubeJS datapack failed to load — likely a syntax error in server_scripts/ or startup_scripts/.",Some("Check kubejs/server_scripts/ for syntax errors. Use Config Editor in TuffBox to validate JSON files."),&[])]
    } else {
        vec![]
    }
}

fn check_language_provider_mismatch(combined: &str) -> Vec<CrashAnalysisFinding> {
    if combined.contains("LanguageProvider") && combined.contains("Mismatch")
        || combined.contains("language provider") && combined.contains("require")
    {
        vec![f(
            "error",
            "LANGUAGE_PROVIDER_MISMATCH",
            "Wrong loader version for a mod",
            "A mod was built for a different Forge/NeoForge version than installed.",
            Some("Download the correct version for your loader."),
            &[],
        )]
    } else {
        vec![]
    }
}

fn check_modernfix_watchdog(combined: &str) -> Vec<CrashAnalysisFinding> {
    if combined.contains("Watchdog") && combined.contains("deadlocked")
        || combined.contains("ModernFix") && combined.contains("Watchdog")
    {
        vec![f(
            "error",
            "MODERNFIX_WATCHDOG",
            "Integrated server deadlocked",
            "ModernFix watchdog detected a server deadlock.",
            Some("Increase watchdog timeout or remove conflicting performance mods."),
            &[],
        )]
    } else {
        vec![]
    }
}

fn check_feature_order_cycle(combined: &str) -> Vec<CrashAnalysisFinding> {
    if combined.contains("FeatureOrderCycle")
        || (combined.contains("feature")
            && combined.contains("cycle")
            && combined.contains("worldgen"))
    {
        vec![f("error","FEATURE_ORDER_CYCLE","Circular worldgen feature dependency","Two worldgen features have circular order dependencies.",Some("Remove one of the conflicting worldgen mods or check their configs for feature order settings."),&[])]
    } else {
        vec![]
    }
}

fn check_medieval_origins(ctx: &AnalysisCtx, combined: &str) -> Vec<CrashAnalysisFinding> {
    let has_medieval = ctx
        .installed_mods
        .iter()
        .any(|m| m.contains("medieval") && m.contains("origin"));
    let has_forge_origins = ctx
        .installed_mods
        .iter()
        .any(|m| m == "origins" || m == "apotheosis");
    if has_medieval && has_forge_origins && combined.contains("origin") {
        vec![f(
            "error",
            "MEDIEVAL_ORIGINS_VS_FORGE",
            "Medieval Origins needs Fabric Origins",
            "Medieval Origins conflicts with Forge Origins — needs Fabric version.",
            Some("Use Fabric Origins, not Forge Origins."),
            &[],
        )]
    } else {
        vec![]
    }
}

fn check_geckolib_oculus(ctx: &AnalysisCtx) -> Vec<CrashAnalysisFinding> {
    if ctx
        .installed_mods
        .contains(&"geckolib_oculus_compat".to_string())
        || ctx
            .installed_mods
            .contains(&"geckolib-oculus-compat".to_string())
    {
        vec![f(
            "info",
            "GECKOLIB_OCULUS_COMPAT",
            "GeckoLib Oculus Compat no longer needed",
            "On 1.20.1+ GeckoLib handles Oculus compatibility natively. This mod causes crashes.",
            Some("Remove GeckoLib Oculus Compat mod."),
            &[],
        )]
    } else {
        vec![]
    }
}

fn check_intel_driver(combined: &str) -> Vec<CrashAnalysisFinding> {
    if combined.to_lowercase().contains("intel")
        && (combined.to_lowercase().contains("driver") || combined.to_lowercase().contains("ig"))
        && combined.to_lowercase().contains("crash")
    {
        vec![f(
            "warning",
            "MODERN_INTEL_DRIVER",
            "Intel GPU driver crash",
            "Crashed inside Intel GPU driver — update drivers.",
            Some("Update Intel GPU drivers from intel.com."),
            &["https://www.intel.com/content/www/us/en/download-center/home.html"],
        )]
    } else {
        vec![]
    }
}

fn check_macos_shader_driver(ctx: &AnalysisCtx, combined: &str) -> Vec<CrashAnalysisFinding> {
    if ctx.os_name.to_lowercase().contains("mac")
        && (combined.contains("shader") || combined.contains("GLSL"))
        && combined.contains("error")
    {
        vec![f(
            "warning",
            "MACOS_SHADER_DRIVER",
            "macOS shader driver issue",
            "Shader-related crash on macOS — common with Iris/Oculus shaderpacks.",
            Some("Try disabling shaders or updating to latest Iris/Oculus nightly."),
            &[],
        )]
    } else {
        vec![]
    }
}

fn check_jvm_dll_error(combined: &str) -> Vec<CrashAnalysisFinding> {
    if combined.contains("jvm.dll")
        || (combined.contains("jvm") && combined.contains(".dll") && combined.contains("error"))
    {
        vec![f(
            "error",
            "JVM_DLL_ERROR",
            "JVM native error",
            "jvm.dll crash — may be caused by corrupted Java, antivirus blocking, or RAM issues.",
            Some("Reinstall Java, check antivirus exclusions, run a memory test."),
            &[],
        )]
    } else {
        vec![]
    }
}

fn check_corrupted_mod_jar(combined: &str) -> Vec<CrashAnalysisFinding> {
    if combined.contains("Invalid zip")
        || combined.contains("invalid CEN header")
        || combined.contains("corrupted jar")
    {
        vec![f(
            "error",
            "CORRUPTED_MOD_JAR",
            "Corrupted mod JAR file",
            "A mod JAR is corrupted — it may have been a failed download.",
            Some("Use 'Repair Profile' or re-download the mod from Modrinth."),
            &[],
        )]
    } else {
        vec![]
    }
}

fn check_watermedia_vlc(combined: &str) -> Vec<CrashAnalysisFinding> {
    if combined.contains("WaterMedia") || combined.contains("vlcj") || combined.contains("VLC") {
        vec![f(
            "error",
            "WATERMEDIA_VLC",
            "WaterMedia needs VLC",
            "WaterMedia mod requires VLC libraries to be installed.",
            Some("Install VLC media player from videolan.org."),
            &["https://www.videolan.org/"],
        )]
    } else {
        vec![]
    }
}

fn check_irlandacore_backdoor(ctx: &AnalysisCtx) -> Vec<CrashAnalysisFinding> {
    if ctx
        .installed_mods
        .iter()
        .any(|m| m.contains("irlandacore") || m.contains("irlanda"))
    {
        vec![f(
            "critical",
            "IRLANDACORE_BACKDOOR",
            "IrlandaCore security risk",
            "IrlandaCore contains a creative mode backdoor for the author. Remove immediately.",
            Some("Remove IrlandaCore and all mods depending on it."),
            &[],
        )]
    } else {
        vec![]
    }
}

fn check_class_metadata_not_found(combined: &str) -> Vec<CrashAnalysisFinding> {
    if combined.contains("ClassMetadataNotFoundException")
        || combined.contains("class metadata not found")
    {
        vec![f(
            "error",
            "CLASS_METADATA_NOT_FOUND",
            "Class metadata missing",
            "Mixin target class not found — version mismatch between mod and Minecraft/loader.",
            Some("Update the mod to a version compatible with your Minecraft version."),
            &[],
        )]
    } else {
        vec![]
    }
}

fn check_mcreator_mods(mods: &[String]) -> Vec<CrashAnalysisFinding> {
    let mcreator = find_mcreator_mods(mods);
    if !mcreator.is_empty() {
        let list = mcreator
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        vec![f(
            "info",
            "MCREATOR_MODS_DETECTED",
            &format!("{} MCreator mod(s) detected", mcreator.len()),
            &format!(
                "MCreator mods: {}. These may have lower quality or compatibility issues.",
                list
            ),
            None,
            &[],
        )]
    } else {
        vec![]
    }
}

// ═══════════════════════════════════════════════════════════════════
// Tool: Package/Class Finder + Jdeps

/// Finds which installed mod provides a given Java class or package.
/// Prefers exact `.class` path match; resolves mod id via jar metadata when possible.
pub fn find_class_in_mods(class_name: &str, mods_dir: &std::path::Path) -> Vec<ClassMatch> {
    let mut results = Vec::new();
    if !mods_dir.is_dir() {
        return results;
    }
    let fqn = class_name.trim().trim_end_matches(".class");
    if fqn.is_empty() {
        return results;
    }
    let exact = format!("{}.class", fqn.replace('.', "/"));
    let package_prefix = {
        let slash = fqn.replace('.', "/");
        if let Some((pkg, _)) = slash.rsplit_once('/') {
            format!("{pkg}/")
        } else {
            String::new()
        }
    };

    for entry in std::fs::read_dir(mods_dir).into_iter().flatten().flatten() {
        let p = entry.path();
        if p.extension().map_or(true, |e| e != "jar") {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy().to_string();
        let Ok(f) = std::fs::File::open(&p) else {
            continue;
        };
        let Ok(zip) = zip::ZipArchive::new(f) else {
            continue;
        };
        let names: Vec<String> = zip.file_names().map(|s| s.to_string()).collect();
        let hit = names.iter().any(|n| n == &exact)
            || (!package_prefix.is_empty()
                && names
                    .iter()
                    .any(|n| n.starts_with(&package_prefix) && n.ends_with(".class")));
        if !hit {
            // Fallback: exact class basename somewhere in the jar (rare shading cases).
            let simple = fqn.rsplit('.').next().unwrap_or(fqn);
            let simple_class = format!("{simple}.class");
            if !names.iter().any(|n| n.ends_with(&simple_class)) {
                continue;
            }
        }

        let meta = crate::mod_scan::scan_mod_jar(&p).ok();
        let mod_id = meta
            .as_ref()
            .and_then(|m| m.mod_id.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| file_name.trim_end_matches(".jar").to_string());
        let mod_name = mod_id.clone();
        results.push(ClassMatch {
            class_name: fqn.to_string(),
            mod_id,
            mod_name,
            file_name: Some(file_name),
        });
    }
    results
}

/// Extract top FQNs from crash text for class→jar attribution (capped).
pub fn extract_blame_class_names(text: &str, limit: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let re_candidates = [
        "java.lang.NoClassDefFoundError:",
        "java.lang.ClassNotFoundException:",
        "Caused by:",
        "Exception in thread",
    ];
    for (idx, line) in text.lines().enumerate() {
        if out.len() >= limit {
            break;
        }
        let trimmed = line.trim();
        // Stack frame: at pkg.Class.method(
        if let Some(rest) = trimmed.strip_prefix("at ") {
            if let Some(fqn) = rest.split('(').next() {
                if let Some((cls, _)) = fqn.rsplit_once('.') {
                    // cls is package.Class or package.Class$Inner
                    let class_fqn = cls.split('$').next().unwrap_or(cls);
                    if class_fqn.contains('.')
                        && !class_fqn.starts_with("java.")
                        && !class_fqn.starts_with("jdk.")
                        && !class_fqn.starts_with("sun.")
                        && !class_fqn.starts_with("net.minecraft.")
                        && !class_fqn.starts_with("com.mojang.")
                        && seen.insert(class_fqn.to_string())
                    {
                        out.push(class_fqn.to_string());
                    }
                }
            }
            continue;
        }
        for prefix in re_candidates {
            if let Some(rest) = trimmed
                .strip_prefix(prefix)
                .or_else(|| {
                    let lower = trimmed.to_lowercase();
                    let p = prefix.to_lowercase();
                    if lower.contains(&p) {
                        trimmed.split(prefix).nth(1)
                    } else {
                        None
                    }
                })
            {
                let token = rest
                    .trim()
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim_matches(|c: char| c == ':' || c == '"' || c == '\'');
                let class_fqn = token.replace('/', ".").split('$').next().unwrap_or(token).to_string();
                if class_fqn.contains('.')
                    && !class_fqn.starts_with("java.")
                    && seen.insert(class_fqn.clone())
                {
                    out.push(class_fqn);
                }
            }
        }
        // Prefer early lines (exception head).
        if idx > 120 && out.len() >= 3 {
            break;
        }
    }
    out.truncate(limit);
    out
}

/// Finds all mods that depend on a class — mirrors Crash Assistant's Jdeps analysis.
/// Searches all JARs for references to the given class.
pub fn find_mods_depending_on_class(
    class_name: &str,
    mods_dir: &std::path::Path,
    _installed: &[String],
) -> Vec<ClassMatch> {
    let mut results = Vec::new();
    if !mods_dir.is_dir() {
        return results;
    }
    let target = class_name.replace('.', "/");
    for entry in std::fs::read_dir(mods_dir).into_iter().flatten().flatten() {
        let p = entry.path();
        if p.extension().map_or(true, |e| e != "jar") {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let mod_id = name.trim_end_matches(".jar").to_string();
        if let Ok(f) = std::fs::File::open(&p) {
            if let Ok(mut zip) = zip::ZipArchive::new(f) {
                // Check if any class inside references the target class
                let mut found = false;
                for i in 0..zip.len() {
                    if let Ok(mut zf) = zip.by_index(i) {
                        if zf.name().ends_with(".class") {
                            let mut buf = Vec::new();
                            if std::io::Read::read_to_end(&mut zf, &mut buf).is_ok() {
                                let text = String::from_utf8_lossy(&buf);
                                if text.contains(&target) {
                                    found = true;
                                    break;
                                }
                            }
                        }
                    }
                }
                if found {
                    results.push(ClassMatch {
                        class_name: class_name.into(),
                        mod_id: mod_id.clone(),
                        mod_name: mod_id,
                        file_name: Some(name),
                    });
                }
            }
        }
    }
    results
}

fn find_classes_in_crashes(_ctx: &AnalysisCtx, combined: &str) -> Vec<ClassMatch> {
    let mut results = Vec::new();
    for line in combined.lines() {
        if line.contains("NoClassDefFoundError") || line.contains("ClassNotFoundException") {
            if let Some(class) = line
                .split(": ")
                .nth(1)
                .and_then(|s| s.split_whitespace().next())
            {
                if class.len() > 5 && class.len() < 200 && class.contains('.') {
                    let _mods_dir = std::path::PathBuf::new(); // caller provides path
                    results.push(ClassMatch {
                        class_name: class.to_string(),
                        mod_id: "?".into(),
                        mod_name: "?".into(),
                        file_name: None,
                    });
                }
            }
        }
    }
    results.truncate(10);
    results
}

// ═══════════════════════════════════════════════════════════════════
// Helpers

fn extract_suspected(ctx: &AnalysisCtx, combined: &str) -> Vec<String> {
    let mut s = match_mods_in_text(combined, &ctx.installed_mods);
    // Prefer mods that appear on real failure lines — not Fabric inventory dumps
    // or harmless "provided by 'modid'" mentions from mixin/debug chatter.
    let error_blob: String = combined
        .lines()
        .filter(|l| {
            let lower = l.to_lowercase();
            if looks_like_mod_inventory_line(l) {
                return false;
            }
            let hard_fail = lower.contains("exception")
                || lower.contains("caused by")
                || lower.contains("could not execute entrypoint")
                || lower.contains("due to errors")
                || lower.contains("mixin apply failed")
                || lower.contains("fatal")
                || (lower.contains("error")
                    && !lower.contains("error_reporter")
                    && !lower.contains("errors=")
                    && !lower.contains("errorlevel")
                    && !lower.contains("no errors"));
            // "provided by" alone is too noisy (normal Fabric logs); only keep
            // it when the same line already looks like a failure.
            hard_fail
        })
        .collect::<Vec<_>>()
        .join("\n");
    if !error_blob.is_empty() {
        let on_errors = match_mods_in_text(&error_blob, &ctx.installed_mods);
        if !on_errors.is_empty() {
            s = on_errors;
        } else {
            // No installed-mod tokens on failure lines → don't invent suspects
            // from the full log (avoids false "fix Critters…" while the pack runs).
            s.clear();
        }
    } else {
        // Healthy / warning-only log: don't surface suspects.
        s.clear();
    }
    s.truncate(10);
    s
}

fn compute_mod_diff(ctx: &AnalysisCtx) -> (Vec<String>, Vec<String>) {
    let added: Vec<_> = ctx
        .installed_mods
        .iter()
        .filter(|m| !ctx.previous_mods.contains(m))
        .cloned()
        .collect();
    let removed: Vec<_> = ctx
        .previous_mods
        .iter()
        .filter(|m| !ctx.installed_mods.contains(m))
        .cloned()
        .collect();
    (added, removed)
}

/// Catalog of real-world Fabric/Forge/NeoForge conflict phrases from latest.log
/// / crash-reports, each mapped to one or more one-click FixActions.
fn check_conflict_log_phrases(ctx: &AnalysisCtx, combined: &str) -> Vec<CrashAnalysisFinding> {
    let mut out = Vec::new();
    let lower = combined.to_lowercase();

    // --- Duplicate mods ---
    if lower.contains("found duplicate mods")
        || lower.contains("duplicatemodsfoundexception")
        || lower.contains("duplicate mods found")
        || lower.contains("failed to build unique mod list")
    {
        let mods = match_mods_in_text(combined, &ctx.installed_mods);
        let evidence = first_evidence_line(
            combined,
            &[
                "Found duplicate mods",
                "Duplicate mods found",
                "DuplicateModsFoundException",
                "Failed to build unique mod list",
            ],
        ).map(truncate_evidence);
        let mut fixes = Vec::new();
        for m in mods.iter().take(4) {
            fixes.push(fix_action(
                "disableMod",
                &format!("Disable duplicate candidate `{m}`"),
                Some(m),
            ));
            fixes.push(fix_action(
                "removeMod",
                &format!("Remove `{m}`"),
                Some(m),
            ));
        }
        out.push(fx(
            "critical",
            "DUPLICATE_MODS",
            "Duplicate mods detected",
            "The loader found two jars claiming the same mod ID (or a dependency jar bundled inside another mod). Keep one copy and remove/disable the rest.",
            Some("Open mods/ and remove the older jar listed next to the mod ID in latest.log."),
            &[],
            fixes,
            evidence,
        ));
    }

    // --- Missing / unmet dependencies ---
    if lower.contains("missing or unsupported mandatory")
        || lower.contains("unmet dependency")
        || lower.contains("which is missing")
        || (lower.contains("requires") && lower.contains("but") && lower.contains("missing"))
        || lower.contains("modresolutionexception")
        || lower.contains("missingdependencyexception")
    {
        let needed = extract_required_mod_ids(combined);
        let evidence = first_evidence_line(
            combined,
            &[
                "requires",
                "which is missing",
                "mandatory dependency",
                "ModResolutionException",
                "MissingDependency",
            ],
        ).map(truncate_evidence);
        let mut fixes = Vec::new();
        let known_deps = [
            "fabric-api",
            "cloth-config",
            "architectury",
            "indium",
            "forgeconfigapiport",
            "kotlin-for-forge",
            "geckolib",
            "playeranimator",
            "moonlight",
            "terrablender",
        ];
        let mut candidates = needed.clone();
        for k in known_deps {
            if lower.contains(k) && !candidates.iter().any(|c| c.eq_ignore_ascii_case(k)) {
                candidates.push(k.to_string());
            }
        }
        for dep in candidates {
            if ctx.installed_mods.iter().any(|m| m.eq_ignore_ascii_case(&dep)) {
                continue;
            }
            fixes.push(fix_action(
                "installDependency",
                &format!("Install missing `{dep}`"),
                Some(&dep),
            ));
        }
        // Also offer updating the dependent mod(s) named in the log.
        for m in match_mods_in_text(combined, &ctx.installed_mods).into_iter().take(3) {
            fixes.push(fix_action(
                "updateMod",
                &format!("Update `{m}` (may change dependency range)"),
                Some(&m),
            ));
        }
        out.push(fx(
            "critical",
            "MISSING_DEPENDENCY",
            "Missing or unmet mod dependency",
            "A mod requires another mod / library that is absent or the wrong version. Fabric usually prints `Mod 'x' requires … which is missing`; Forge prints mandatory dependency tables.",
            Some("Install the named dependency for the same Minecraft + loader version."),
            &["https://minefixtools.com/fixes/how-to-fix-missing-mods-on-server"],
            fixes,
            evidence,
        ));
    }

    // --- Wrong loader / platform mismatch ---
    if lower.contains("is for forge")
        || lower.contains("is for fabric")
        || (lower.contains("requires forge") && lower.contains("fabric"))
        || lower.contains("mod file is for forge, but this is fabric")
        || lower.contains("mod file is for fabric, but this is forge")
        || lower.contains("incompatiblemodsexception")
        || lower.contains("wrong loader")
        || (lower.contains("quilt") && lower.contains("requires fabric") && lower.contains("missing"))
    {
        let mods = match_mods_in_text(combined, &ctx.installed_mods);
        let evidence = first_evidence_line(
            combined,
            &[
                "is for Forge",
                "is for Fabric",
                "Mod file is for",
                "IncompatibleModsException",
                "wrong loader",
            ],
        ).map(truncate_evidence);
        let mut fixes: Vec<_> = mods
            .iter()
            .take(4)
            .flat_map(|m| {
                vec![
                    fix_action("disableMod", &format!("Disable wrong-loader `{m}`"), Some(m)),
                    fix_action("removeMod", &format!("Remove `{m}`"), Some(m)),
                ]
            })
            .collect();
        fixes.push(fix_action(
            "updateLoader",
            "Update loader to latest for this Minecraft version",
            None,
        ));
        out.push(fx(
            "critical",
            "WRONG_LOADER",
            "Mod built for a different loader",
            "A jar targets Forge/Fabric/NeoForge/Quilt while this instance uses another loader. File extension `.jar` does not identify the platform.",
            Some("Remove the mismatched jar or switch the instance loader."),
            &["https://minefixtools.com/fixes/how-to-fix-mod-version-mismatch"],
            fixes,
            evidence,
        ));
    }

    // --- Version mismatch Minecraft / loader ---
    if lower.contains("requires minecraft")
        || (lower.contains("minecraft version")
            && (lower.contains("incompatible") || lower.contains("mismatch")))
        || lower.contains("requires fabricloader")
        || (lower.contains("requires forge") && lower.contains(">="))
        || lower.contains("unsupported minecraft version")
    {
        let mods = match_mods_in_text(combined, &ctx.installed_mods);
        let evidence = first_evidence_line(
            combined,
            &[
                "requires Minecraft",
                "requires fabricloader",
                "Unsupported Minecraft",
                "incompatible with",
            ],
        ).map(truncate_evidence);
        let mut fixes: Vec<_> = mods
            .iter()
            .take(4)
            .map(|m| {
                fix_action(
                    "updateMod",
                    &format!("Update `{m}` for MC {}", ctx.mc_version),
                    Some(m),
                )
            })
            .collect();
        fixes.push(fix_action(
            "updateLoader",
            "Update Fabric/Forge/NeoForge loader",
            None,
        ));
        out.push(fx(
            "error",
            "VERSION_MISMATCH",
            "Minecraft / loader version mismatch",
            "A mod declares a Minecraft or loader version range that does not include this instance. Updating the mod or the loader usually fixes it.",
            Some(&format!(
                "Match all mods to Minecraft {} + {} {}.",
                ctx.mc_version, ctx.loader, ctx.loader_version
            )),
            &[],
            fixes,
            evidence,
        ));
    }

    // --- NoSuchMethod / NoSuchField (API break) ---
    if lower.contains("nosuchmethoderror")
        || lower.contains("nosuchfielderror")
        || lower.contains("abstractmethoderror")
        || lower.contains("incompatibleclasschangeerror")
    {
        let mods = match_mods_in_text(combined, &ctx.installed_mods);
        let evidence = first_evidence_line(
            combined,
            &[
                "NoSuchMethodError",
                "NoSuchFieldError",
                "AbstractMethodError",
                "IncompatibleClassChangeError",
            ],
        ).map(truncate_evidence);
        let fixes: Vec<_> = mods
            .iter()
            .take(5)
            .flat_map(|m| {
                vec![
                    fix_action("updateMod", &format!("Update `{m}`"), Some(m)),
                    fix_action("disableMod", &format!("Disable `{m}` to test"), Some(m)),
                ]
            })
            .collect();
        out.push(fx(
            "error",
            "API_BREAK",
            "Broken mod API (NoSuchMethod/NoSuchField)",
            "Code expected a method/field that another mod or Minecraft no longer provides — classic sign of mismatched Fabric API / library / Minecraft version.",
            Some("Update Fabric API and the mods named in the stacktrace together."),
            &[],
            fixes,
            evidence,
        ));
    }

    // --- Entrypoint / ModLoadingException ---
    if lower.contains("entrypointexception")
        || (lower.contains("failed to start") && lower.contains("entrypoint"))
        || lower.contains("modloadingexception")
        || lower.contains("error loading mods")
        || lower.contains("failed to load mods")
    {
        let mods = match_mods_in_text(combined, &ctx.installed_mods);
        let evidence = first_evidence_line(
            combined,
            &[
                "EntrypointException",
                "entrypoint",
                "ModLoadingException",
                "Error loading mods",
                "Failed to load mods",
            ],
        ).map(truncate_evidence);
        let fixes: Vec<_> = mods
            .iter()
            .take(4)
            .flat_map(|m| {
                vec![
                    fix_action("disableMod", &format!("Disable `{m}`"), Some(m)),
                    fix_action("updateMod", &format!("Update `{m}`"), Some(m)),
                    fix_action("reinstallMod", &format!("Reinstall `{m}`"), Some(m)),
                ]
            })
            .collect();
        out.push(fx(
            "critical",
            "MOD_LOADING_FAILURE",
            "Mod failed during loading / entrypoint",
            "The loader could not initialize a mod (constructor, initializer, or Forge event bus). The first `Caused by` under EntrypointException / ModLoadingException usually names the culprit.",
            Some("Disable the named mod, then update or replace it."),
            &["https://minefixtools.com/fixes/how-to-read-fabric-crash-reports"],
            fixes,
            evidence,
        ));
    }

    // --- Duplicate classes / ASM on classpath ---
    if (lower.contains("duplicate") && lower.contains("classpath"))
        || lower.contains("duplicate asm")
        || lower.contains("verifyclasspath")
        || lower.contains("duplicate classes found")
    {
        let evidence = first_evidence_line(
            combined,
            &["duplicate", "classpath", "ASM", "LoaderUtil.verifyClasspath"],
        ).map(truncate_evidence);
        out.push(fx(
            "error",
            "DUPLICATE_CLASSPATH",
            "Duplicate libraries on classpath",
            "Two versions of the same library (often ASM) are on the JVM classpath. Common after loader/Minecraft library version drift.",
            Some("Reinstall the loader profile / clear libraries cache, then relaunch."),
            &[],
            vec![fix_action(
                "updateLoader",
                "Re-resolve loader (update loader version)",
                None,
            )],
            evidence,
        ));
    }

    // --- JEI / REI conflict ---
    if (lower.contains("jei") && lower.contains("rei") && (lower.contains("duplicate") || lower.contains("conflict")))
        || lower.contains("reiplugincompatibilities") && lower.contains("jei")
    {
        let mut fixes = Vec::new();
        if ctx.installed_mods.iter().any(|m| m.eq_ignore_ascii_case("jei")) {
            fixes.push(fix_action("disableMod", "Disable JEI (keep REI)", Some("jei")));
        }
        if ctx.installed_mods.iter().any(|m| m.eq_ignore_ascii_case("roughlyenoughitems") || m.eq_ignore_ascii_case("rei"))
        {
            fixes.push(fix_action(
                "disableMod",
                "Disable REI (keep JEI)",
                Some("roughlyenoughitems"),
            ));
        }
        out.push(fx(
            "warning",
            "JEI_REI_CONFLICT",
            "JEI and REI conflict",
            "JEI and Roughly Enough Items both provide recipe UIs; some compat jars also claim the `jei` mod ID and trigger duplicate-mod errors.",
            Some("Use either JEI or REI (+ REI Plugin Compatibilities), not both."),
            &[],
            fixes,
            first_evidence_line(combined, &["jei", "rei", "duplicate"]).map(truncate_evidence),
        ));
    }

    // --- Out of memory ---
    if lower.contains("outofmemoryerror")
        || lower.contains("java heap space")
        || lower.contains("gc overhead limit")
    {
        out.push(fx(
            "critical",
            "OUT_OF_MEMORY",
            "Out of memory",
            "The JVM exhausted heap memory. Heavily modded packs often need 4–8 GB.",
            Some("Raise allocated memory, then retest."),
            &[],
            vec![fix_action(
                "raiseMemory",
                "Raise allocated memory to 6 GB",
                None,
            )],
            first_evidence_line(combined, &["OutOfMemoryError", "Java heap space", "GC overhead"]).map(truncate_evidence),
        ));
    }

    // --- Mixin apply failed (structured fixes; complements check_mixins) ---
    if lower.contains("mixin apply failed")
        || (lower.contains("@mixin") && lower.contains("failed"))
        || lower.contains("mixinprepareerror")
        || lower.contains("invalidinjectionexception")
    {
        let mods = match_mods_in_text(combined, &ctx.installed_mods);
        let evidence = first_evidence_line(
            combined,
            &[
                "Mixin apply failed",
                "MixinPrepareError",
                "InvalidInjectionException",
                "mixin",
            ],
        ).map(truncate_evidence);
        let fixes: Vec<_> = mods
            .iter()
            .take(5)
            .flat_map(|m| {
                vec![
                    fix_action("updateMod", &format!("Update `{m}`"), Some(m)),
                    fix_action("disableMod", &format!("Disable `{m}`"), Some(m)),
                ]
            })
            .collect();
        if !fixes.is_empty() {
            out.push(fx(
                "error",
                "MIXIN_CONFLICT",
                "Mixin apply failed (mod conflict / version)",
                "A mod tried to inject into Minecraft code and failed — wrong MC version, two mods editing the same target, or a library mismatch. Not usually a broken Fabric loader itself.",
                Some("Update or disable the mods named next to the mixin config in the log."),
                &["https://minefixtools.com/fixes/how-to-read-fabric-crash-reports"],
                fixes,
                evidence,
            ));
        }
    }

    // --- Access transformer / IllegalAccess ---
    if lower.contains("illegalaccessexception")
        || lower.contains("inaccessibleobjectexception")
        || lower.contains("module java.base does not")
    {
        out.push(fx(
            "warning",
            "MODULE_ACCESS",
            "Java module access error",
            "A mod or library tried to reflect into a sealed JDK module. Often fixed by Java flags or using a supported Java major for this Minecraft version.",
            Some("Ensure the Project Java matches the version recommended for this Minecraft release."),
            &[],
            vec![],
            first_evidence_line(
                combined,
                &[
                    "IllegalAccessException",
                    "InaccessibleObjectException",
                    "does not export",
                ],
            ).map(truncate_evidence),
        ));
    }

    // --- Corrupted / zip errors ---
    if lower.contains("zipexception")
        || lower.contains("invalid cen header")
        || lower.contains("unexpected end of zlib")
        || (lower.contains("truncat") && lower.contains(".jar"))
    {
        let mods = match_mods_in_text(combined, &ctx.installed_mods);
        let fixes: Vec<_> = mods
            .iter()
            .take(3)
            .map(|m| fix_action("reinstallMod", &format!("Re-download `{m}`"), Some(m)))
            .collect();
        out.push(fx(
            "error",
            "CORRUPT_JAR",
            "Corrupted mod jar",
            "A jar failed ZIP/deflate validation — incomplete download or disk corruption.",
            Some("Re-download the named jar(s)."),
            &[],
            fixes,
            first_evidence_line(combined, &["ZipException", "CEN header", "zlib", ".jar"]).map(truncate_evidence),
        ));
    }

    // --- Non-unique Mixin config (two mods share the same mixins.json name) ---
    if lower.contains("non-unique mixin config") {
        let evidence = first_evidence_line(combined, &["Non-unique Mixin config", "non-unique mixin"]).map(truncate_evidence);
        let mut mods = match_mods_in_text(combined, &ctx.installed_mods);
        // Also pull "used by the mods A and B" tokens when present.
        if let Some(ev) = evidence.as_ref() {
            if let Some(idx) = ev.to_lowercase().find("used by the mods") {
                let tail = &ev[idx + "used by the mods".len()..];
                for token in tail
                    .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
                    .filter(|t| t.len() > 1)
                {
                    if !mods.iter().any(|m| m.eq_ignore_ascii_case(token)) {
                        mods.push(token.to_string());
                    }
                }
            }
        }
        let fixes: Vec<_> = mods
            .iter()
            .take(4)
            .flat_map(|m| {
                vec![
                    fix_action(
                        "disableMod",
                        &format!("Disable `{m}` (duplicate mixin config)"),
                        Some(m),
                    ),
                    fix_action("removeMod", &format!("Remove `{m}`"), Some(m)),
                ]
            })
            .collect();
        out.push(fx(
            "critical",
            "NONUNIQUE_MIXIN_CONFIG",
            "Non-unique Mixin config name",
            "Two mods ship the same `*.mixins.json` config name (e.g. thiccpackets vs xlpackets). Fabric refuses to start until one is removed.",
            Some("Remove one of the conflicting mods; only the mod authors can rename the shared config."),
            &["https://github.com/FabricMC/fabric-loader/issues/834"],
            fixes,
            evidence,
        ));
    }

    // --- Forge "mod has failed to load correctly" ---
    if lower.contains("has failed to load correctly")
        || lower.contains("error has occurred during loading")
        || lower.contains("1 error has occurred during loading")
    {
        let mods = match_mods_in_text(combined, &ctx.installed_mods);
        let fixes: Vec<_> = mods
            .iter()
            .take(4)
            .flat_map(|m| {
                vec![
                    fix_action("updateMod", &format!("Update `{m}`"), Some(m)),
                    fix_action("disableMod", &format!("Disable `{m}`"), Some(m)),
                    fix_action("reinstallMod", &format!("Reinstall `{m}`"), Some(m)),
                ]
            })
            .collect();
        out.push(fx(
            "critical",
            "MOD_FAILED_LOAD",
            "Mod failed to load correctly",
            "Forge/NeoForge reported a mod init failure — often a missing dependency, duplicate jar, corrupt download, or loader mismatch.",
            Some("Check the first Caused by under the failed mod, then update/disable that mod."),
            &[],
            fixes,
            first_evidence_line(
                combined,
                &[
                    "has failed to load correctly",
                    "error has occurred during loading",
                ],
            ).map(truncate_evidence),
        ));
    }

    // --- UnsupportedClassVersionError (wrong Java) ---
    if lower.contains("unsupportedclassversionerror") {
        out.push(fx(
            "critical",
            "WRONG_JAVA_VERSION",
            "Wrong Java version (UnsupportedClassVersionError)",
            "A class was compiled for a newer Java than the runtime provides. Modern Minecraft/Fabric often needs Java 17 or 21.",
            Some("Point the project at a supported Java major for this Minecraft version."),
            &["https://minefixtools.com/fixes/how-to-read-fabric-crash-reports"],
            vec![],
            first_evidence_line(combined, &["UnsupportedClassVersionError"]).map(truncate_evidence),
        ));
    }

    out
}

fn find_mcreator_mods(mods: &[String]) -> Vec<String> {
    // Heuristic: MCreator mods often have "mod", "mcreator" patterns,
    // or common MCreator mod naming signatures
    let mcreator_patterns = [
        "mcreator_",
        "_mcreator",
        "mcr_",
        "mod_mcreator",
        "examplemod",
        "testmod",
        "mymod",
    ];
    mods.iter()
        .filter(|m| {
            let lower = m.to_lowercase();
            mcreator_patterns.iter().any(|p| lower.contains(p))
        })
        .cloned()
        .collect()
}

fn build_message(ctx: &AnalysisCtx, findings: &[CrashAnalysisFinding], platform: &str) -> String {
    let mut msg = String::new();
    let errs: Vec<_> = findings
        .iter()
        .filter(|f| f.severity == "critical" || f.severity == "error")
        .collect();
    let warns: Vec<_> = findings
        .iter()
        .filter(|f| f.severity == "warning")
        .collect();

    if platform == "discord" {
        msg.push_str("**TuffBox Crash Assistant**\n\n");
        msg.push_str(&format!(
            "**MC:** {} | **Loader:** {} {} | **Java:** {} | **OS:** {}\n\n",
            ctx.mc_version, ctx.loader, ctx.loader_version, ctx.java_version, ctx.os_name
        ));
        if !errs.is_empty() {
            msg.push_str("### Errors\n");
            for e in &errs {
                msg.push_str(&format!("- **{}**: {}\n", e.code, e.title));
            }
        }
        if !warns.is_empty() {
            msg.push_str("\n### Warnings\n");
            for w in &warns {
                msg.push_str(&format!("- **{}**: {}\n", w.code, w.title));
            }
        }
        if errs.is_empty() && warns.is_empty() {
            msg.push_str("No known crash patterns detected.\n");
        }
    } else {
        msg.push_str("## TuffBox Crash Assistant\n\n");
        msg.push_str("| Property | Value |\n|---|---|\n");
        msg.push_str(&format!("| MC | {} |\n", ctx.mc_version));
        msg.push_str(&format!(
            "| Loader | {} {} |\n",
            ctx.loader, ctx.loader_version
        ));
        msg.push_str(&format!("| Java | {} |\n", ctx.java_version));
        msg.push_str(&format!("| OS | {} |\n", ctx.os_name));
        if !errs.is_empty() {
            msg.push_str("\n### Errors\n");
            for e in errs {
                msg.push_str(&format!("- **{}**: {}\n", e.code, e.title));
            }
        }
    }
    msg
}

/// Read the last `max_lines` of a log file as a single string.
pub fn tail_log(path: &Path, max_lines: usize) -> String {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().collect();
            let start = lines.len().saturating_sub(max_lines);
            lines[start..].join("\n")
        }
        Err(_) => String::new(),
    }
}

/// Analyze a failed launch log and produce a categorized, user-facing launch
/// error the UI surfaces with a Retry action. The logic runs the same
/// crash-analysis engine used for the in-app report, but is kept tiny and
/// synchronous so it can be exercised in unit tests.
pub fn classify_launch_crash(
    log_path: &Path,
    exit_code: Option<i32>,
    mc_version: &str,
    java_version: &str,
    loader_kind: &str,
    loader_version: &str,
    installed_mods: &[String],
) -> LaunchErrorInfo {
    let tail = tail_log(log_path, 300);
    let analysis_ctx = AnalysisCtx {
        crash_content: tail.lines().map(|s| s.to_string()).collect(),
        latest_log: tail.clone(),
        launcher_log: String::new(),
        installed_mods: installed_mods.to_vec(),
        previous_mods: Vec::new(),
        java_version: java_version.to_string(),
        java_vendor: String::new(),
        os_name: std::env::consts::OS.to_string(),
        mc_version: mc_version.to_string(),
        loader: loader_kind.to_string(),
        loader_version: loader_version.to_string(),
        cpu_name: String::new(),
        gpu_names: Vec::new(),
        total_ram_mb: 0,
        is_offline: false,
        win_events: Vec::new(),
    };

    let report = run_full_analysis(&analysis_ctx);

    let code_note = match exit_code {
        Some(c) => format!("Minecraft exited with code {c}. "),
        None => "Minecraft exited unexpectedly. ".to_string(),
    };

    let mut message = code_note;
    if report.findings.is_empty() {
        message.push_str("No obvious cause was detected in the log — open the log to investigate.");
    } else {
        // Surface the most severe findings (up to 3) directly in the toast so
        // the user sees the likely causes without first opening Diagnose.
        let severity_rank = |s: &str| match s {
            "critical" => 3,
            "high" => 2,
            "medium" => 1,
            _ => 0,
        };
        let mut ranked: Vec<&CrashAnalysisFinding> = report.findings.iter().collect();
        ranked.sort_by(|a, b| severity_rank(&b.severity).cmp(&severity_rank(&a.severity)));
        let shown = ranked.len().min(3);
        message.push_str("Likely cause(s): ");
        for (i, f) in ranked.iter().take(shown).enumerate() {
            if i > 0 {
                message.push_str("; ");
            }
            message.push_str(&f.title);
        }
        if report.findings.len() > shown {
            message.push_str(&format!(
                ". +{} more — open Diagnose for the full report.",
                report.findings.len() - shown
            ));
        } else {
            message.push('.');
        }
    }

    LaunchErrorInfo::new(LaunchErrorKind::LaunchCrash, message).with_log(log_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp_log(name: &str, contents: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("tuffbox_crash_test");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(contents.as_bytes()).unwrap();
        p
    }

    #[test]
    fn classify_launch_crash_marks_nonzero_exit_as_launch_crash() {
        let log = write_temp_log(
            "crash.log",
            "java.lang.OutOfMemoryError: Java heap space\n\tat net.minecraft.server.MinecraftServer",
        );
        let info = classify_launch_crash(
            &log,
            Some(1),
            "1.20.1",
            "17.0.1",
            "fabric",
            "0.15.0",
            &[],
        );
        assert_eq!(info.kind, LaunchErrorKind::LaunchCrash);
        assert!(!info.message.is_empty());
        assert!(info.retryable());
        assert_eq!(info.log_path.as_deref(), Some(log.to_str().unwrap()));
    }

    #[test]
    fn classify_launch_crash_handles_missing_log_gracefully() {
        let missing = std::env::temp_dir()
            .join("tuffbox_crash_test")
            .join("does_not_exist.log");
        let info =
            classify_launch_crash(&missing, Some(1), "1.20.1", "17", "vanilla", "", &[]);
        assert_eq!(info.kind, LaunchErrorKind::LaunchCrash);
        assert!(info.retryable());
        assert_eq!(info.log_path.as_deref(), Some(missing.to_str().unwrap()));
    }

    #[test]
    fn classify_launch_crash_lists_likely_causes() {
        let log = write_temp_log(
            "lock.log",
            "java.nio.file.FileSystemException: run/locks: The process cannot access the file because it is being used by another process",
        );
        let info = classify_launch_crash(&log, Some(1), "1.20.1", "17", "fabric", "0.15.0", &[]);
        assert_eq!(info.kind, LaunchErrorKind::LaunchCrash);
        assert!(info.message.contains("Likely cause(s):"));
        assert!(info.message.contains("File locked by another process"));
    }

    fn ctx() -> AnalysisCtx {
        AnalysisCtx {
            crash_content: vec![
                "NoClassDefFoundError: com/example/Foo".into(),
                "Mixin apply failed: sodium.mixins.json".into(),
            ],
            latest_log: "[ERROR] Create: NoSuchMethod on create_connected\n".into(),
            launcher_log: String::new(),
            installed_mods: vec![
                "sodium".into(),
                "iris".into(),
                "create".into(),
                "create_connected".into(),
            ],
            previous_mods: vec!["sodium".into(), "create".into()],
            java_version: "17".into(),
            java_vendor: "Adoptium".into(),
            os_name: "Windows 11".into(),
            mc_version: "1.20.1".into(),
            loader: "fabric".into(),
            loader_version: "0.15".into(),
            cpu_name: "Intel i9-13900K".into(),
            gpu_names: vec!["Intel UHD".into(), "NVIDIA RTX 4090".into()],
            total_ram_mb: 32768,
            is_offline: false,
            win_events: vec![],
        }
    }
    #[test]
    fn detects_mixin() {
        assert!(!check_mixins(
            &ctx(),
            &(ctx().crash_content.join("\n") + "\n" + &ctx().latest_log)
        )
        .is_empty());
    }
    #[test]
    fn detects_intel() {
        assert!(!check_intel_cpu(&ctx()).is_empty());
    }
    #[test]
    fn detects_gpu() {
        assert!(!check_integrated_gpu(&ctx()).is_empty());
    }
    #[test]
    fn detects_module() {
        assert!(!check_module_resolution(&ctx(), &ctx().crash_content.join("\n")).is_empty());
    }
    #[test]
    fn detects_create_addons() {
        assert!(!check_create6_addons(&ctx(), &ctx().latest_log).is_empty());
    }
    #[test]
    fn detects_suspects() {
        let s = extract_suspected(&ctx(), &ctx().crash_content.join("\n"));
        assert!(s.contains(&"sodium".into()));
    }
    #[test]
    fn full_report() {
        let r = run_full_analysis(&ctx());
        assert!(r.findings.len() >= 3);
    }

    #[test]
    fn detects_duplicate_mods_phrase() {
        let mut c = ctx();
        c.latest_log = "[main/ERROR]: Found duplicate mods:\n\tMod ID: 'sodium' from mod files: a.jar, b.jar\n"
            .into();
        let hits = check_conflict_log_phrases(&c, &c.latest_log);
        assert!(hits.iter().any(|f| f.code == "DUPLICATE_MODS"));
        let f = hits.iter().find(|f| f.code == "DUPLICATE_MODS").unwrap();
        assert!(!f.fixes.is_empty());
        assert!(f.fixes.iter().any(|a| a.kind == "disableMod" || a.kind == "removeMod"));
    }

    #[test]
    fn detects_missing_dependency_phrase() {
        let mut c = ctx();
        c.installed_mods = vec!["create".into()];
        c.latest_log =
            "Mod 'create' requires 'fabric-api' which is missing!\nModResolutionException\n".into();
        let hits = check_conflict_log_phrases(&c, &c.latest_log);
        assert!(hits.iter().any(|f| f.code == "MISSING_DEPENDENCY"));
        let f = hits.iter().find(|f| f.code == "MISSING_DEPENDENCY").unwrap();
        assert!(f
            .fixes
            .iter()
            .any(|a| a.kind == "installDependency" && a.mod_id.as_deref() == Some("fabric-api")));
    }

    #[test]
    fn detects_nonunique_mixin_config() {
        let mut c = ctx();
        c.installed_mods = vec!["thiccpackets".into(), "xlpackets".into()];
        c.latest_log = "java.lang.RuntimeException: Non-unique Mixin config name xlpackets.mixins.json used by the mods thiccpackets and xlpackets\n".into();
        let hits = check_conflict_log_phrases(&c, &c.latest_log);
        assert!(hits.iter().any(|f| f.code == "NONUNIQUE_MIXIN_CONFIG"));
        let f = hits
            .iter()
            .find(|f| f.code == "NONUNIQUE_MIXIN_CONFIG")
            .unwrap();
        assert!(f.fixes.iter().any(|a| a.mod_id.as_deref() == Some("thiccpackets")));
    }

    #[test]
    fn detects_oom_with_raise_memory_fix() {
        let mut c = ctx();
        c.latest_log = "java.lang.OutOfMemoryError: Java heap space\n".into();
        let hits = check_conflict_log_phrases(&c, &c.latest_log);
        let f = hits.iter().find(|f| f.code == "OUT_OF_MEMORY").unwrap();
        assert!(f.fixes.iter().any(|a| a.kind == "raiseMemory"));
    }

    #[test]
    fn class_finder_matches_exact_class_and_mod_id() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("coolmod-1.0.jar");
        {
            let file = std::fs::File::create(&jar).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("fabric.mod.json", opts).unwrap();
            zip.write_all(
                br#"{"schemaVersion":1,"id":"coolmod","version":"1","authors":["Zed"]}"#,
            )
            .unwrap();
            zip.start_file("com/example/CoolClass.class", opts).unwrap();
            zip.write_all(&[0u8; 8]).unwrap();
            zip.finish().unwrap();
        }
        let hits = find_class_in_mods("com.example.CoolClass", dir.path());
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].mod_id, "coolmod");
        assert_eq!(hits[0].file_name.as_deref(), Some("coolmod-1.0.jar"));

        let names = extract_blame_class_names(
            "java.lang.NoClassDefFoundError: com/example/CoolClass\n\tat com.example.CoolClass.init(CoolClass.java:10)\n",
            5,
        );
        assert!(names.iter().any(|n| n == "com.example.CoolClass"));
    }
}
