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

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashAnalysisFinding {
    pub severity: String,
    pub code: String,
    pub title: String,
    pub description: String,
    pub auto_fix: Option<String>,
    pub references: Vec<String>,
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
pub struct ClassMatch {
    pub class_name: String,
    pub mod_id: String,
    pub mod_name: String,
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
    CrashAnalysisFinding {
        severity: severity.into(),
        code: code.into(),
        title: title.into(),
        description: description.into(),
        auto_fix: auto_fix.map(|s| s.into()),
        references: refs.iter().map(|s| s.to_string()).collect(),
    }
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
        let mut suspect = String::new();
        for m in &ctx.installed_mods {
            if combined.to_lowercase().contains(&m.to_lowercase()) {
                suspect = m.clone();
                break;
            }
        }
        let mut refs = Vec::new();
        // Find all mods that appear near the mixin failure
        let mut seen = std::collections::HashSet::new();
        for line in combined
            .lines()
            .filter(|l| l.contains("Mixin") || l.contains("mixin"))
        {
            for m in &ctx.installed_mods {
                if line.to_lowercase().contains(&m.to_lowercase()) && seen.insert(m) {
                    refs.push(m.clone());
                }
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
        vec![f(
            "error",
            "MIXIN_APPLY_FAILED",
            &title_s,
            &desc_s,
            fix_s.as_deref(),
            &[],
        )]
    } else {
        vec![]
    }
}

fn check_missing_mods(ctx: &AnalysisCtx, combined: &str) -> Vec<CrashAnalysisFinding> {
    let mut out = Vec::new();
    let has = |s: &str| ctx.installed_mods.contains(&s.to_string());
    if has("sodium") && !has("indium") && ctx.loader == "fabric" {
        out.push(f(
            "error",
            "MISSING_INDIUM",
            "Indium is missing",
            "Sodium on Fabric needs Indium for Fabric Renderer API.",
            Some("Install Indium from Modrinth."),
            &["https://modrinth.com/mod/indium"],
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
/// This mirrors Crash Assistant's Package/Class Finder GUI tool.
pub fn find_class_in_mods(class_name: &str, mods_dir: &std::path::Path) -> Vec<ClassMatch> {
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
            if let Ok(zip) = zip::ZipArchive::new(f) {
                let found = zip.file_names().any(|fname| fname.contains(&target));
                if found {
                    results.push(ClassMatch {
                        class_name: class_name.into(),
                        mod_id: mod_id.clone(),
                        mod_name: mod_id,
                    });
                }
            }
        }
    }
    results
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
    let mut s = Vec::new();
    for m in &ctx.installed_mods {
        if combined.to_lowercase().contains(&m.to_lowercase()) && !s.contains(m) {
            s.push(m.clone());
        }
    }
    s.truncate(15);
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

#[cfg(test)]
mod tests {
    use super::*;
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
}
