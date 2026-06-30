# 04. Карта функций TuffBox

## P0 — основа IDE

- Project workspace.
- Manifest.
- Lockfile.
- Profiles: client, server, dev, release.
- Mod list.
- Modrinth import.
- Local jar import.
- Dependency graph.
- Missing dependency detection.
- Basic conflict detection.
- Snapshots.
- Rollback.
- Test launch.
- Crash log collection.
- Basic config editor.
- Export `.mrpack`.

## P1 — сильный MVP

- AI crash explanation.
- Change plan before applying fixes.
- Safe updates / risky updates.
- Diff between snapshots.
- Changelog generation.
- Export Prism instance.
- Server/client side labeling.
- Search across configs.
- Formatting JSON/TOML.

## P2 — профессиональная версия

- CurseForge import/export.
- Server pack builder.
- KubeJS snippets.
- CraftTweaker snippets.
- Migration advisor: Forge → NeoForge.
- Compatibility database.
- Performance audit.
- Test matrix.
- GitHub Releases export.
- Modrinth draft publishing.

## P3 — ecosystem

- Cloud sharing.
- Team collaboration.
- Branded player launcher.
- Server profiles.
- Discord bot integration.
- Public project pages.
- Analytics for pack versions.

## Главный пользовательский цикл

```text
Create project
→ Add mods
→ Resolve dependencies
→ Edit configs
→ Create snapshot
→ Test run
→ Analyze crash if failed
→ Apply fix plan
→ Test again
→ Release snapshot
→ Export
```
