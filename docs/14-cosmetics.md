# TuffBox Cosmetics

Appearance system for Me + in-game: HD skin/cape, wings, hats, trails, jump/combat FX; Mojang / Ely.by / TLauncher via CustomSkinLoader; TuffBox-only sharing of custom looks.

## Layers

1. **Me → Appearance** — upload skin/cape, pick wings/hat, FX toggles ([`Me.svelte`](../apps/tuffbox-desktop/src/components/Me.svelte)).
2. **Launch inject** — [`cosmetics_runtime.rs`](../crates/tuffbox-core/src/cosmetics_runtime.rs) injects CSL + nearest **anchor** cosmetics jar, writes `.tuffbox/cosmetics-session.json`.
3. **Supabase** — `cosmetics_profiles` + Storage; edge `cosmetics-get` / `cosmetics-upsert`.
4. **Client mod** — [`bridges/cosmetics`](../bridges/cosmetics): `core` (Java 8 protocol) + per-anchor platform jars.

## CSL load order

1. TuffBox CustomSkinAPI  
2. Mojang → Ely.by → TLauncher → LocalSkin  

## Version matrix (anchors + fallback)

Inject selects the **highest anchor ≤ pack MC** that supports the loader (Quilt → Fabric).

| Anchor | Forge | Fabric | NeoForge |
|--------|-------|--------|----------|
| 1.12.2 | scaffold | — | — |
| 1.16.5 | scaffold | scaffold | — |
| 1.20.1 | scaffold | scaffold | scaffold |
| **1.21.1** | — | **full FX** | session/core |
| 1.21.4 | — | scaffold | scaffold |
| 1.21.11 | — | scaffold | scaffold |
| 26.1.2 | — | scaffold | scaffold |
| 26.2 | — | scaffold | scaffold |

Examples: `1.20.4+fabric` → `1.20.1`; `1.21.3` → `1.21.1`; `1.21.5` → `1.21.4`; `26.1.0` → `1.21.11`; `1.12.2+fabric` → no FX jar (CSL/session only).

Details: [`bridges/cosmetics/MATRIX.md`](../bridges/cosmetics/MATRIX.md).

Build: `cd bridges/cosmetics && gradlew syncDesktopResources` (JDK 21; `core` uses JDK 8 toolchain).

## Privacy

- Vanilla / other launchers without our inject do **not** see TuffBox custom cosmetics.
- Mojang / Ely / TLauncher skins are visible to anyone running CSL.

## References

| Project | Role |
|---------|------|
| [CustomSkinLoader](https://modrinth.com/mod/customskinloader) | Injected skin/cape loader |
| [Cosmetica 2](https://github.com/Cosmetica-cc/Cosmetica-2) (Apache-2.0) | Feature-layer patterns — do **not** vendor jar |
| [Soup Visuals](https://github.com/Pa-dej/soup-visuals-beta-3.0.0) | **Inspiration only** (SOUP-1.0 ARR — no copy) |

See [`bridges/cosmetics/THIRD_PARTY.md`](../bridges/cosmetics/THIRD_PARTY.md).

## Deploy

```bash
supabase db push   # 013_cosmetics_profiles.sql
supabase functions deploy cosmetics-get --no-verify-jwt
supabase functions deploy cosmetics-upsert --no-verify-jwt
```
