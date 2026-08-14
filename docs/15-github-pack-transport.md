# 15. GitHub Pack Transport

Repo-native distribution of TuffBox packs through **public GitHub repositories**. Isolated from TuffSwarm (`docs/13-tuffswarm-network.md`). Friends install and update **anonymously** — no Microsoft, Modrinth, or GitHub account. Only the **author** authenticates (device OAuth or a PAT fallback).

This increment does **not** include private repos, PR/fork flow, `tuffbox://` deep links, server AutoModpack sync, ads, or antivirus scanning.

## Repository contract

A published pack is a Git tree, not a zip-only GitHub Release:

| Path | Role |
|------|------|
| `{id}.tuffbox.json` | Canonical TuffBox manifest (source of truth) |
| `{id}.tuffbox.lock.json` | Lockfile beside the manifest |
| `pack.toml` / `index.toml` / `mods/*.pw.toml` | packwiz interop (MC + loader live in `[versions]`) |
| overrides (`config/`, `kubejs/`, …) | Copied when present |
| `README.md` | Share instructions |
| `.tuffbox/repo-transport.json` | Transport metadata (`ready` / `publishing`, `managedFiles`, digest, signature) |

There is no separate `modpack.json` or `options.toml`. Custom jars under 50 MiB stay in git; larger jars are GitHub Release assets with a two-phase `publishing` → `ready` marker. Consumers ignore non-`ready` revisions.

Local install state is **never** published. It lives at `.tuffbox/transport.json` on the player machine (repo, ref, commit, version, pinned signer).

## Publish

1. Stage a deterministic tree (`stage_repo_tree`).
2. Sign the pretty-printed sidecar manifest with a local Ed25519 key (OS keyring).
3. Git Data API: blobs → tree (`base_tree` preserves foreign files) → one commit → fast-forward ref. `managedFiles` lets TuffBox delete only files it previously generated.
4. Identical `contentDigest` is a no-op. Non-fast-forward updates are reported as conflicts; TuffBox never force-pushes.

Author login uses GitHub device flow (`public_repo` scope). Only the public OAuth client ID is shipped (`TUFFBOX_GITHUB_CLIENT_ID`).

## Consume

1. Parse `owner/repo`, `gh:owner/repo:tag`, or `https://github.com/owner/repo`.
2. Inspect the public repo (name, version, `ready` / `publishing`) before install. Revisions still marked `publishing` are refused.
3. Download the commit tarball anonymously, extract with path-traversal limits, prefer the sidecar manifest (packwiz is the fallback).
4. TOFU-pin the author public key; reject silent signer changes.
5. Updates: semantic diff (add/remove/bump, MC/loader, overrides, custom vs provider origin) → confirm → snapshot with `managed_files` → apply → SHA-1/SHA-512 verify → rollback the snapshot on failure.

Minecraft or loader changes require a full rematerialize (`requiresFullReinstall`): obsolete provider jars from the previous pack are removed, then missing files are downloaded through `mod_files`.

## Desktop API

`api.transport.github` in `apps/tuffbox-desktop/src/lib/api.ts`: parse, inspect, device login, stage preview, publish, install, check/preview/apply update. Library import previews the public repo then installs anonymously; Release Room publishes the pack tree with progress phases and conflict handling; an IDE banner shows a readable content diff, custom-file/signer warnings, and rollback snapshot id.

## Trust

- Modrinth/CurseForge files: known-origin CDN, hashes in the manifest.
- Custom GitHub jars/assets: untrusted custom content, shown in the update preview.
- Ed25519 sidecar signature is optional for unsigned packs; when present it is verified on import.

## Patterns (not GPL code)

Prism: stage → validate → atomic apply. Modrinth Shared Instances: semantic diff → confirm → snapshot → incremental apply. TuffBox implements those flows against `{id}.tuffbox.json`, not against Prism/Modrinth sources.
