//! Deterministic mod-version requirement parsing and checking.
//!
//! When a loader refuses to start because "mod X requires version R of Y",
//! the fix is computable from the error text alone — no AI needed. This
//! module parses those requirements (Fabric `requires version 'R' of 'Y'`,
//! Forge/NeoForge `Mod ID / Requested by / Expected range / Actual version`)
//! and verifies installed versions against constraints in code
//! ([`version_satisfies`]).
//!
//! Both the Crash Assistant findings and the `changeModVersion` fix executor
//! share this logic: parse → verify → resolve newest satisfying release.

use std::cmp::Ordering;

/// One loader-stated version constraint: `requester` needs `dep_id` at
/// `constraint` (raw text, e.g. `0.6.0`, `>=1.2`, `[0.6,)`, `^0.15`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepVersionReq {
    /// Mod that declares the requirement ("" when the line names none).
    pub requester: String,
    /// Required mod id, lowercased.
    pub dep_id: String,
    /// Raw constraint text, trimmed.
    pub constraint: String,
    /// Version the loader saw (`wrong version is present` / `Actual version`).
    pub found: Option<String>,
    /// Loader says the dep is absent entirely (`which is missing`).
    pub missing: bool,
}

/// Parse loader version-requirement lines from combined crash/log text.
///
/// Handles Fabric (`Mod 'x' requires version 'R' of mod 'y'`) and
/// Forge/NeoForge (`Mod ID: 'y', Requested by: 'x', Expected range: 'R',
/// Actual version: 'V'`) shapes, single or double quotes. Lines without a
/// version-shaped constraint are skipped, as are `java`/`minecraft`/loader
/// pseudo-deps (those belong to the Java/MC checks, not mod pins).
pub fn parse_dep_version_requirements(combined: &str) -> Vec<DepVersionReq> {
    let mut out = Vec::new();
    for line in combined.lines() {
        // Parse values from the lowercased line: every downstream use (mod
        // ids, constraints, found versions) compares case-insensitively, and
        // slicing the original at lowercased indices would panic on non-ASCII
        // text (lowercasing shifts byte offsets).
        let lower = line.to_lowercase();
        if let Some(req) = parse_forge_range_line(&lower) {
            push_dedup(&mut out, req);
            continue;
        }
        if let Some(req) = parse_fabric_requires_line(&lower) {
            push_dedup(&mut out, req);
        }
    }
    out.truncate(8);
    out
}

fn push_dedup(out: &mut Vec<DepVersionReq>, req: DepVersionReq) {
    if !out.iter().any(|r| r.dep_id == req.dep_id && r.constraint == req.constraint) {
        out.push(req);
    }
}

/// Pseudo-dependencies that must never become mod version pins.
fn is_skipped_dep(id: &str) -> bool {
    matches!(
        id,
        "java" | "minecraft" | "fabricloader" | "fabric-loader" | "forge" | "neoforge" | "quilt"
            | "quiltloader" | "quilt-loader" | "loader" | "modloader" | "openjdk"
    )
}

/// Mod-id shaped: short, lowercase alnum with `-_.`, no spaces.
fn looks_like_mod_id(s: &str) -> bool {
    let s = s.trim();
    if s.len() < 2 || s.len() > 64 || s.contains(' ') {
        return false;
    }
    s.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && s.chars().any(|c| c.is_ascii_alphabetic())
}

/// Constraint shaped: starts version-ish, version charset only, has a digit
/// or is `*`. Rejects prose like `25 or later` (spaces + stray words are
/// stripped first, then letters outside `v`/`x` fail the check).
fn looks_like_constraint(s: &str) -> bool {
    let c: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if c.is_empty() || c.len() > 64 {
        return false;
    }
    let mut chars = c.chars();
    let first = chars.next().unwrap_or('\0');
    if !(first.is_ascii_digit()
        || matches!(first, 'v' | 'V' | '*' | '[' | '(' | '<' | '>' | '=' | '!' | '^' | '~'))
    {
        return false;
    }
    let mut has_digit_or_star = false;
    for ch in c.chars() {
        if ch.is_ascii_digit() || ch == '*' {
            has_digit_or_star = true;
        } else if !(ch.is_ascii_alphanumeric()
            || matches!(
                ch,
                '.' | '_' | '-' | '+' | '[' | ']' | '(' | ')' | '<' | '>' | '=' | '!' | '^'
                    | '~' | ','
                    | '|'
            )) {
            return false;
        } else if ch.is_ascii_alphabetic() && !matches!(ch, 'v' | 'V' | 'x' | 'X') {
            return false;
        }
    }
    has_digit_or_star
}

/// Extract the next single/double/back quoted segment starting at/after `from`.
/// Returns (value, index just past the closing quote).
fn next_quoted(line: &str, from: usize) -> Option<(String, usize)> {
    let bytes = line.as_bytes();
    let mut i = from;
    while i < bytes.len() {
        let q = bytes[i];
        if q == b'\'' || q == b'"' || q == b'`' {
            if let Some(rel) = line[i + 1..].find(q as char) {
                return Some((line[i + 1..i + 1 + rel].to_string(), i + 1 + rel + 1));
            }
            return None;
        }
        i += 1;
    }
    None
}

/// `Mod 'x' requires version 'R' of mod 'y'[, but only the wrong version is
/// present: 'V'!]` / `… which is missing!` / `depends on version 'R' of 'y'`.
fn parse_fabric_requires_line(lower: &str) -> Option<DepVersionReq> {
    let anchor = ["requires version", "depends on version"]
        .iter()
        .filter_map(|a| lower.find(a).map(|i| (i, *a)))
        .min_by_key(|(i, _)| *i)?;
    let (anchor_idx, anchor_text) = anchor;
    // Requester: `mod 'x'` before the anchor, else a lone quoted token right
    // before `requires` (`'x' requires …`).
    let before = &lower[..anchor_idx];
    let requester = mod_id_before(before).unwrap_or_default();
    // Constraint: first quoted segment after the anchor.
    let after_anchor = anchor_idx + anchor_text.len();
    let (constraint_raw, next) = next_quoted(lower, after_anchor)?;
    let constraint = constraint_raw.trim().to_string();
    if !looks_like_constraint(&constraint) {
        return None;
    }
    // Dep: `of (mod )?'y'` after the constraint.
    let rest = &lower[next..];
    let of_idx = rest.find(" of ")? + next;
    let (dep_raw, dep_end) = next_quoted(lower, of_idx + 4)?;
    let dep_id = dep_raw.trim().to_string();
    if !looks_like_mod_id(&dep_id) || is_skipped_dep(&dep_id) {
        return None;
    }
    let tail = &lower[dep_end..];
    let missing = tail.contains("which is missing") || tail.contains("is missing!");
    let found = parse_found_version(tail);
    Some(DepVersionReq {
        requester,
        dep_id,
        constraint,
        found,
        missing,
    })
}

/// `mod 'x'` (or `"x"`) in `before`; else trailing `'x'` right before the end.
/// `before` must already be lowercased (see [`parse_dep_version_requirements`]).
fn mod_id_before(before: &str) -> Option<String> {
    if let Some(idx) = before.rfind("mod ") {
        if let Some((id, _)) = next_quoted(before, idx + 4) {
            let id = id.trim().to_string();
            if looks_like_mod_id(&id) {
                return Some(id);
            }
        }
    }
    // `'x' requires …` — last quoted token in `before`.
    let mut last: Option<String> = None;
    let mut from = 0;
    while let Some((q, next)) = next_quoted(before, from) {
        let q = q.trim().to_string();
        if looks_like_mod_id(&q) {
            last = Some(q);
        }
        from = next;
    }
    last
}

/// `but only the wrong version is present: 'V'!` / `actual version: 'V'`.
fn parse_found_version(tail: &str) -> Option<String> {
    for key in ["wrong version is present:", "actual version:"] {
        if let Some(idx) = tail.find(key) {
            let after = tail[idx + key.len()..].trim();
            let v: String = after
                .trim_start_matches(['\'', '"', '`'])
                .chars()
                .take_while(|c| {
                    !c.is_whitespace() && !matches!(*c, '!' | '\'' | '"' | '`' | ',' | ';')
                })
                .collect();
            let v = v.trim().to_string();
            if v.chars().any(|c| c.is_ascii_digit()) && v.len() <= 64 {
                return Some(v);
            }
        }
    }
    None
}

/// `Mod ID: 'y', Requested by: 'x', Expected range: 'R'[, Actual version: 'V']`
/// (Forge/NeoForge; quote style and key order may vary).
fn parse_forge_range_line(lower: &str) -> Option<DepVersionReq> {
    if !(lower.contains("mod id") && lower.contains("expected range")) {
        return None;
    }
    let dep = key_quoted(lower, "mod id")?;
    let dep_id = dep.trim().to_string();
    if !looks_like_mod_id(&dep_id) || is_skipped_dep(&dep_id) {
        return None;
    }
    let constraint = key_quoted(lower, "expected range")?;
    let constraint = constraint.trim().to_string();
    if !looks_like_constraint(&constraint) {
        return None;
    }
    // `Requested by` may itself be a comma/and list — take the first id-like token.
    let requester = key_quoted(lower, "requested by")
        .map(|r| {
            r.split([',', '&', '+'])
                .filter_map(|p| {
                    let p = p.trim().trim_matches(['\'', '"', '`']).to_string();
                    looks_like_mod_id(&p).then_some(p)
                })
                .next()
                .unwrap_or_default()
        })
        .unwrap_or_default();
    let found = key_quoted(lower, "actual version")
        .map(|v| v.trim().to_string())
        .filter(|v| v.chars().any(|c| c.is_ascii_digit()) && v.len() <= 64);
    // Computed before the literal moves `found` (E0382 otherwise).
    let missing = found.is_none() && lower.contains("missing");
    Some(DepVersionReq {
        requester,
        dep_id,
        constraint,
        found,
        missing,
    })
}

/// Value of a `Key: 'value'` (or `"value"`) pair on the line.
/// `lower` must already be lowercased (see [`parse_dep_version_requirements`]).
fn key_quoted(lower: &str, key: &str) -> Option<String> {
    let idx = lower.find(key)?;
    let after_key = &lower[idx + key.len()..];
    let colon = after_key.find(':')?;
    let abs = idx + key.len() + colon + 1;
    // Prefer a quoted segment; else take a bare token up to `,`/whitespace.
    if let Some((q, _)) = next_quoted(lower, abs) {
        // Only accept the quote if no `,` intervenes — otherwise the quote
        // belongs to a later pair.
        let between = &lower[abs..];
        let qpos = between.find(['\'', '"', '`']).unwrap_or(usize::MAX);
        let comma = between.find(',').unwrap_or(usize::MAX);
        if qpos < comma {
            return Some(q);
        }
    }
    let rest = lower[abs..].trim();
    // Bare bracketed range keeps its inner comma: `Expected range: [0.6,)`.
    // Either closing bracket ends it (`[` pairs with `]` or `)` in Maven ranges).
    if rest.starts_with('[') || rest.starts_with('(') {
        if let Some(end) = rest.find([')', ']']) {
            return Some(rest[..=end].to_string());
        }
    }
    let bare: String = rest
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != ',')
        .collect();
    let bare = bare.trim().trim_matches(['\'', '"', '`']).to_string();
    if bare.is_empty() {
        None
    } else {
        Some(bare)
    }
}

// ═══════════════════════════════════════════════════════════════════
// Version comparison + constraint satisfaction (code-verified, no AI).

/// Numeric dotted comparison with semver hygiene: `+build` metadata ignored,
/// `-qualifier` sorts below the release (`1.0` > `1.0-beta`), leading `v`
/// tolerated. Returns `None` when neither side has any digits (uncomparable).
///
/// Note: no Minecraft-prefix stripping here — [`version_satisfies`] retries
/// with the stripped form instead, so ambiguous `1.20.0-2`-style versions
/// compare exactly first and loosely second.
pub fn compare_versions(a: &str, b: &str) -> Option<Ordering> {
    let (ra, pre_a) = numeric_runs(a);
    let (rb, pre_b) = numeric_runs(b);
    if ra.is_empty() && rb.is_empty() {
        return None;
    }
    let len = ra.len().max(rb.len());
    for i in 0..len {
        let x = ra.get(i).copied().unwrap_or(0);
        let y = rb.get(i).copied().unwrap_or(0);
        if x != y {
            return Some(x.cmp(&y));
        }
    }
    // Same numeric core: release beats pre-release (`1.0` > `1.0-beta`).
    Some(pre_b.cmp(&pre_a))
}

/// Numeric runs of a version core + whether a `-qualifier` was cut.
fn numeric_runs(v: &str) -> (Vec<u64>, bool) {
    // Build metadata never affects precedence: `0.6.2+mc1.20.1` == `0.6.2`.
    let core = v.split('+').next().unwrap_or(v);
    let core = strip_leading_v(core.trim());
    let (core, pre) = match core.split_once('-') {
        Some((c, _)) => (c, true),
        None => (core, false),
    };
    let runs: Vec<u64> = core
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.parse::<u64>().ok())
        .collect();
    (runs, pre)
}

fn strip_leading_v(s: &str) -> &str {
    s.strip_prefix('v')
        .or_else(|| s.strip_prefix('V'))
        .unwrap_or(s)
}

/// Strip a leading Minecraft version mistaken for the mod version:
/// `mc1.20.1-0.6.2` → `0.6.2`, `1.21.4-0.6.4` → `0.6.4`. Only strips when the
/// remainder starts with a digit. The bare-`1.` form is ambiguous with
/// genuine `1.20.0-2`-style build suffixes — acceptable, because
/// [`version_satisfies`] only consults the stripped form when the exact form
/// fails to satisfy.
fn strip_mc_prefix(s: &str) -> &str {
    let t = s.trim();
    // Literal `mc1.20.1-…`.
    if let Some(rest) = t
        .strip_prefix("mc")
        .or_else(|| t.strip_prefix("MC"))
        .and_then(|r| r.find(['-', '_']).map(|i| &r[i + 1..]))
    {
        if rest.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            return rest;
        }
    }
    // Bare `1.21.4-…` (modern MC is 1.x).
    if t.starts_with("1.") && t.len() > 4 {
        if let Some(dash) = t.find(['-', '_']) {
            let (head, rest) = (&t[..dash], &t[dash + 1..]);
            if rest.chars().next().is_some_and(|c| c.is_ascii_digit())
                && !head.is_empty()
                && head.split('.').all(|p| p.chars().all(|c| c.is_ascii_digit()))
            {
                return rest;
            }
        }
    }
    t
}

/// True when `installed` satisfies `constraint`; `None` when either side is
/// unparseable (caller must treat as "cannot verify", never as satisfied).
///
/// Supported constraints: exact (`1.2.3`, `=1.2.3`), `*`, comparators
/// (`>=1.2`, `<2.0`, `!=1.0`), Maven ranges (`[1.0,2.0)`, `[1.0,)`,
/// `(,2.0]`), caret (`^1.2.3`), tilde (`~1.2`), wildcards (`1.7.x`),
/// `1.2+` (>=), `||` alternatives (any) and space/comma conjunctions (all).
pub fn version_satisfies(installed: &str, constraint: &str) -> Option<bool> {
    let t = constraint.trim();
    // Length cap: `||`/conjunction arms recurse, so a hostile multi-KB
    // constraint (e.g. an AI-hallucinated version) must not blow the stack.
    if t.is_empty() || t.len() > 256 {
        return None;
    }
    // Digit-less installed versions (`nogits`, `unknown`) are unparseable:
    // "cannot verify", never a verdict (comparing them as 0.0.0 would raise
    // bogus "verified wrong" findings).
    if !installed.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    // `||` alternatives: satisfied when any arm holds.
    if t.contains("||") {
        let mut any_unknown = false;
        for arm in t.split("||") {
            match version_satisfies(installed, arm) {
                Some(true) => return Some(true),
                Some(false) => {}
                None => any_unknown = true,
            }
        }
        return if any_unknown { None } else { Some(false) };
    }
    // Space/comma conjunctions (all must hold) — split BEFORE whitespace is
    // stripped; bracketed Maven ranges keep their inner comma.
    let bracketed = t.starts_with('[') || t.starts_with('(');
    if !bracketed {
        let parts: Vec<&str> = t
            .split([',', ' ', '\t'])
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect();
        if parts.len() > 1 {
            let mut any_unknown = false;
            for part in parts {
                match version_satisfies(installed, part) {
                    Some(true) => {}
                    Some(false) => return Some(false),
                    None => any_unknown = true,
                }
            }
            return if any_unknown { None } else { Some(true) };
        }
    }
    let c: String = t.chars().filter(|c| !c.is_whitespace()).collect();
    // Exact form first; when it fails, retry with a possible MC prefix
    // stripped (`1.21.4-0.6.4` installs satisfy `0.6.x` constraints).
    // `Some(true)` wins from either form; otherwise prefer the exact verdict.
    let full = satisfies_single(installed, &c);
    if full == Some(true) {
        return full;
    }
    let stripped = strip_mc_prefix(installed);
    if stripped == installed.trim() {
        return full;
    }
    let alt = satisfies_single(stripped, &c);
    if alt == Some(true) {
        return alt;
    }
    full.or(alt)
}

fn satisfies_single(installed: &str, c: &str) -> Option<bool> {
    if c == "*" || c.eq_ignore_ascii_case("x") {
        return Some(true);
    }
    // Wildcard prefix: `1.7.x`, `1.7.*`.
    if let Some(prefix) = c
        .strip_suffix(".x")
        .or_else(|| c.strip_suffix(".X"))
        .or_else(|| c.strip_suffix(".*"))
    {
        return wildcard_satisfies(installed, prefix);
    }
    // Trailing `+`: `1.2+` means >= 1.2.
    if let Some(min) = c.strip_suffix('+') {
        return compare_versions(installed, min).map(|o| o != Ordering::Less);
    }
    if let Some(v) = c.strip_prefix(">=") {
        return compare_versions(installed, v).map(|o| o != Ordering::Less);
    }
    if let Some(v) = c.strip_prefix("<=") {
        return compare_versions(installed, v).map(|o| o != Ordering::Greater);
    }
    if let Some(v) = c.strip_prefix("==") {
        return compare_versions(installed, v).map(|o| o == Ordering::Equal);
    }
    if let Some(v) = c.strip_prefix('=') {
        return compare_versions(installed, v).map(|o| o == Ordering::Equal);
    }
    if let Some(v) = c.strip_prefix("!=").or_else(|| c.strip_prefix('!')) {
        return compare_versions(installed, v).map(|o| o != Ordering::Equal);
    }
    if let Some(v) = c.strip_prefix('>') {
        return compare_versions(installed, v).map(|o| o == Ordering::Greater);
    }
    if let Some(v) = c.strip_prefix('<') {
        return compare_versions(installed, v).map(|o| o == Ordering::Less);
    }
    if let Some(v) = c.strip_prefix('^') {
        return caret_satisfies(installed, v);
    }
    if let Some(v) = c.strip_prefix('~') {
        return tilde_satisfies(installed, v);
    }
    if (c.starts_with('[') || c.starts_with('(')) && (c.ends_with(']') || c.ends_with(')')) {
        return maven_range_satisfies(installed, c);
    }
    // Bare exact version (`1.0-beta` included). The digit gate rejects prose;
    // `compare_versions` returns None for digit-less installed versions.
    if c.chars().any(|ch| ch.is_ascii_digit()) {
        return compare_versions(installed, c).map(|o| o == Ordering::Equal);
    }
    None
}

fn wildcard_satisfies(installed: &str, prefix: &str) -> Option<bool> {
    let (runs, _) = numeric_runs(installed);
    let (want, _) = numeric_runs(prefix);
    if runs.is_empty() || want.is_empty() {
        return None;
    }
    Some(
        want.iter()
            .enumerate()
            .all(|(i, w)| runs.get(i).copied().unwrap_or(0) == *w),
    )
}

/// `^1.2.3` := >=1.2.3 <2.0.0; `^0.2.3` := >=0.2.3 <0.3.0; `^0.0.3` := =0.0.3-ish.
fn caret_satisfies(installed: &str, base: &str) -> Option<bool> {
    let (runs, _) = numeric_runs(base);
    if runs.is_empty() {
        return None;
    }
    if compare_versions(installed, base)? == Ordering::Less {
        return Some(false);
    }
    // Upper bound: bump the leftmost non-zero run (or the last run;
    // `runs` is non-empty here, so `len() - 1` cannot underflow).
    let mut upper = runs.clone();
    let bump = upper.iter().position(|&n| n > 0).unwrap_or(upper.len() - 1);
    upper.truncate(bump + 1);
    upper[bump] += 1;
    let upper_s = upper
        .iter()
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(".");
    compare_versions(installed, &upper_s).map(|o| o == Ordering::Less)
}

/// `~1.2.3` := >=1.2.3 <1.3.0; `~1.2` := >=1.2 <1.3; `~1` := >=1 <2.
fn tilde_satisfies(installed: &str, base: &str) -> Option<bool> {
    let (runs, _) = numeric_runs(base);
    if runs.is_empty() {
        return None;
    }
    if compare_versions(installed, base)? == Ordering::Less {
        return Some(false);
    }
    let mut upper = runs.clone();
    if upper.len() == 1 {
        upper[0] += 1;
    } else {
        upper.truncate(2);
        upper[1] += 1;
    }
    let upper_s = upper
        .iter()
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(".");
    compare_versions(installed, &upper_s).map(|o| o == Ordering::Less)
}

/// Maven `[a,b]` / `(a,b)` / `[a,)` / `(,b]` ranges (either bound may be empty).
fn maven_range_satisfies(installed: &str, c: &str) -> Option<bool> {
    let inner = &c[1..c.len() - 1];
    let (lo, hi) = inner.split_once(',')?;
    let lo_inc = c.starts_with('[');
    let hi_inc = c.ends_with(']');
    let lo = lo.trim();
    let hi = hi.trim();
    if !lo.is_empty() {
        let o = compare_versions(installed, lo)?;
        if o == Ordering::Less || (!lo_inc && o == Ordering::Equal) {
            return Some(false);
        }
    }
    if !hi.is_empty() {
        let o = compare_versions(installed, hi)?;
        if o == Ordering::Greater || (!hi_inc && o == Ordering::Equal) {
            return Some(false);
        }
    }
    Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fabric_requires_with_wrong_version() {
        let log = "- Mod 'iris' requires version '0.6.0' of mod 'sodium', but only the wrong version is present: '0.5.0+mc1.20.1'!";
        let reqs = parse_dep_version_requirements(log);
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].requester, "iris");
        assert_eq!(reqs[0].dep_id, "sodium");
        assert_eq!(reqs[0].constraint, "0.6.0");
        assert_eq!(reqs[0].found.as_deref(), Some("0.5.0+mc1.20.1"));
        assert!(!reqs[0].missing);
    }

    #[test]
    fn parses_fabric_missing_with_range() {
        let log = "Mod \"oculus\" requires version \">=1.6\" of mod \"embeddium\" which is missing!";
        let reqs = parse_dep_version_requirements(log);
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].dep_id, "embeddium");
        assert_eq!(reqs[0].constraint, ">=1.6");
        assert!(reqs[0].missing);
    }

    #[test]
    fn parses_forge_expected_range() {
        let log = "Missing or unsupported mandatory dependencies:\nMod ID: 'sodium', Requested by: 'iris', Expected range: '[0.6,)', Actual version: '0.5.0'";
        let reqs = parse_dep_version_requirements(log);
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].requester, "iris");
        assert_eq!(reqs[0].dep_id, "sodium");
        assert_eq!(reqs[0].constraint, "[0.6,)");
        assert_eq!(reqs[0].found.as_deref(), Some("0.5.0"));
    }

    #[test]
    fn parses_unquoted_forge_range() {
        let log = "Mod ID: sodium, Requested by: iris, Expected range: [0.6,)";
        let reqs = parse_dep_version_requirements(log);
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].constraint, "[0.6,)");
        assert_eq!(reqs[0].dep_id, "sodium");
    }

    #[test]
    fn non_ascii_text_does_not_panic() {
        // Lowercasing shifts byte offsets — slicing must stay in one string.
        let log = "Краш мода 'iris' requires version '0.6.0' of mod 'sodium' 💥";
        let reqs = parse_dep_version_requirements(log);
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].dep_id, "sodium");
        assert_eq!(reqs[0].requester, "iris");
    }

    #[test]
    fn skips_java_and_prose_lines() {
        // Java requirement line must NOT become a mod pin (java check owns it).
        let java = "requires version 25 or later of 'Java HotSpot(TM) 64-Bit Server VM' (java)";
        assert!(parse_dep_version_requirements(java).is_empty());
        // Bare prose without quotes/constraints.
        assert!(parse_dep_version_requirements("Mod loading requires a restart of the game").is_empty());
        // Minecraft pseudo-dep.
        let mc = "Mod 'x' requires version '1.20.1' of 'minecraft'";
        assert!(parse_dep_version_requirements(mc).is_empty());
    }

    #[test]
    fn satisfies_exact_and_comparators() {
        assert_eq!(version_satisfies("0.6.2", "0.6.2"), Some(true));
        assert_eq!(version_satisfies("0.6.2+mc1.20.1", "0.6.2"), Some(true));
        assert_eq!(version_satisfies("0.6.3", "0.6.2"), Some(false));
        assert_eq!(version_satisfies("0.6.2", ">=0.6"), Some(true));
        assert_eq!(version_satisfies("0.5.0", ">=0.6"), Some(false));
        assert_eq!(version_satisfies("2.5", "(,2.0)"), Some(false));
        assert_eq!(version_satisfies("1.9", "(,2.0)"), Some(true));
        assert_eq!(version_satisfies("v1.2.3", "1.2.3"), Some(true));
    }

    #[test]
    fn satisfies_ranges_caret_tilde_wildcard() {
        assert_eq!(version_satisfies("0.6.13", "[0.6,)"), Some(true));
        assert_eq!(version_satisfies("0.5.8", "[0.6,)"), Some(false));
        assert_eq!(version_satisfies("1.5", "[1.0,2.0)"), Some(true));
        assert_eq!(version_satisfies("2.0", "[1.0,2.0)"), Some(false));
        assert_eq!(version_satisfies("1.9.0", "^1.2.3"), Some(true));
        assert_eq!(version_satisfies("2.0.0", "^1.2.3"), Some(false));
        assert_eq!(version_satisfies("0.15.11", "^0.15"), Some(true));
        assert_eq!(version_satisfies("0.16.0", "^0.15"), Some(false));
        assert_eq!(version_satisfies("1.2.9", "~1.2.3"), Some(true));
        assert_eq!(version_satisfies("1.3.0", "~1.2.3"), Some(false));
        assert_eq!(version_satisfies("1.7.10", "1.7.x"), Some(true));
        assert_eq!(version_satisfies("1.8.0", "1.7.x"), Some(false));
        assert_eq!(version_satisfies("0.6.0", "*"), Some(true));
        assert_eq!(version_satisfies("1.4", ">=1.0 <2.0"), Some(true));
        assert_eq!(version_satisfies("2.4", ">=1.0 <2.0"), Some(false));
        assert_eq!(version_satisfies("3.0", "1.0 || 3.0"), Some(true));
    }

    #[test]
    fn mc_prefix_and_prerelease_hygiene() {
        assert_eq!(version_satisfies("mc1.20.1-0.6.2", "0.6.2"), Some(true));
        assert_eq!(version_satisfies("1.21.4-0.6.4", ">=0.6"), Some(true));
        assert_eq!(version_satisfies("1.0-beta", "1.0"), Some(false));
        // Exact pre-release constraint: `1.0` is NEWER than `1.0-beta`,
        // so it does not equal it (semver precedence).
        assert_eq!(version_satisfies("1.0", "1.0-beta"), Some(false));
        assert_eq!(version_satisfies("nogits", "1.0"), None);
        assert_eq!(version_satisfies("1.0", "soon"), None);
        // Hostile-length constraints are rejected, never recursed into.
        assert_eq!(version_satisfies("1.0", &"1.0 || ".repeat(100)), None);
    }
}
