//! TuffSwarm Hub — shared HTTP store for ExperienceCapsules.
//!
//! Clients publish sanitized crash→fix capsules (fingerprint + solution + actions).
//! Raw crash logs and author notes are rejected / stripped.
//!
//! Endpoints:
//! - GET  /health
//! - POST /v1/crash/capsules
//! - POST /v1/crash/lookup
//! - POST /v1/crash/diagnose
//! - POST /v1/mods/cooccurrence

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use serde::Serialize;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tuffbox_core::crash_remote::{
    CrashDiagnoseRequest, CrashDiagnoseResponse, CrashLookupRequest, CrashLookupResponse,
};
use tuffbox_core::swarm::{CapsuleLibrary, CooccurrenceStore, ExperienceCapsule, ModPairStat};

#[derive(Debug, Parser)]
#[command(name = "tuffswarm-hub")]
#[command(about = "Shared TuffSwarm hub for durable crash→fix capsules (no raw logs)")]
struct Args {
    /// Listen address (default 0.0.0.0:8787 so LAN clients can join).
    #[arg(long, default_value = "0.0.0.0:8787")]
    bind: String,
    /// Directory for durable JSONL capsule store.
    #[arg(long)]
    data_dir: Option<PathBuf>,
}

struct HubState {
    library: Mutex<CapsuleLibrary>,
    cooccur_path: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "tuffswarm_hub=info,tower_http=info".into()),
        )
        .init();

    let args = Args::parse();
    let data_dir = args.data_dir.unwrap_or_else(default_data_dir);
    std::fs::create_dir_all(&data_dir)?;
    let capsules_path = data_dir.join("capsules.jsonl");
    let cooccur_path = data_dir.join("cooccurrence.json");
    tracing::info!(?data_dir, "TuffSwarm hub data directory");

    let state = Arc::new(HubState {
        library: Mutex::new(CapsuleLibrary::open(capsules_path)),
        cooccur_path,
    });

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/crash/capsules", post(publish_capsule))
        .route("/v1/crash/lookup", post(lookup))
        .route("/v1/crash/diagnose", post(diagnose))
        .route("/v1/mods/cooccurrence", post(cooccurrence))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = args.bind.parse()?;
    tracing::info!(%addr, "listening — point TuffBox Settings → TuffSwarm hub URL here");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn default_data_dir() -> PathBuf {
    dirs::data_dir()
        .or_else(dirs::config_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("TuffBox")
        .join("tuffswarm-hub")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Health {
    ok: bool,
    service: &'static str,
    capsule_count: usize,
}

async fn health(State(state): State<Arc<HubState>>) -> impl IntoResponse {
    let count = state
        .library
        .lock()
        .map(|lib| lib.load_all().len())
        .unwrap_or(0);
    Json(Health {
        ok: true,
        service: "tuffswarm-hub",
        capsule_count: count,
    })
}

async fn publish_capsule(
    State(state): State<Arc<HubState>>,
    Json(body): Json<Value>,
) -> Result<impl IntoResponse, (StatusCode, Json<Value>)> {
    // Reject payloads that try to ship raw logs.
    if body.get("rawLog").is_some()
        || body.get("rawLogs").is_some()
        || body.get("crashReport").is_some()
        || body.get("latestLog").is_some()
        || body
            .pointer("/privacy/rawLogs")
            .and_then(|v| v.as_bool())
            == Some(true)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "raw logs are not accepted — publish ExperienceCapsule only (fingerprint + solution + actions)"
            })),
        ));
    }
    if body.get("notes").is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "author notes must not be published to the swarm hub" })),
        ));
    }

    let capsule = ExperienceCapsule::from_public_value(&body).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e })),
        )
    })?;

    let stored = {
        let lib = state.library.lock().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "hub lock poisoned" })),
            )
        })?;
        lib.publish(&capsule).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": e })),
            )
        })?
    };

    tracing::info!(id = %stored.id, key = %stored.fingerprint.key, "capsule stored");
    Ok((
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "id": stored.id,
            "fingerprintKey": stored.fingerprint.key,
            "successScore": stored.success_score,
            "successCount": stored.success_count,
        })),
    ))
}

async fn lookup(
    State(state): State<Arc<HubState>>,
    Json(req): Json<CrashLookupRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<Value>)> {
    let hay = req.excerpt.clone().unwrap_or_default();
    let limit = req.limit.max(1).min(20) as usize;
    let hits = {
        let lib = state.library.lock().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "hub lock poisoned" })),
            )
        })?;
        lib.lookup(&req.fingerprint, &hay, limit)
    };
    Ok(Json(CrashLookupResponse {
        kb_version: Some(format!("hub-{}", tuffbox_core::time_util::compact_now())),
        hits,
    }))
}

async fn diagnose(
    State(state): State<Arc<HubState>>,
    Json(req): Json<CrashDiagnoseRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<Value>)> {
    let hay = req.excerpt.clone().unwrap_or_default();
    let plan = {
        let lib = state.library.lock().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "hub lock poisoned" })),
            )
        })?;
        lib.diagnose_best(&req.fingerprint, &hay)
    };
    let Some(mut plan) = plan else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "no matching capsule for this fingerprint" })),
        ));
    };
    plan.source = Some("swarm".into());
    Ok(Json(CrashDiagnoseResponse {
        plan,
        kb_version: Some(format!("hub-{}", tuffbox_core::time_util::compact_now())),
        used_llm: false,
    }))
}

async fn cooccurrence(
    State(state): State<Arc<HubState>>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    // Accept optional upload of pairs; always return current top pairs.
    if let Some(pairs) = body.get("pairs").and_then(|v| v.as_array()) {
        let mut store = load_hub_cooccur(&state.cooccur_path);
        if let Some(mc) = body.get("mcVersion").and_then(|v| v.as_str()) {
            store.mc_version = mc.to_string();
        }
        if let Some(loader) = body.get("loader").and_then(|v| v.as_str()) {
            store.loader = loader.to_string();
        }
        for p in pairs {
            let a = p
                .get("modA")
                .or_else(|| p.get("mod_a"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let b = p
                .get("modB")
                .or_else(|| p.get("mod_b"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let count = p.get("count").and_then(|v| v.as_u64()).unwrap_or(1);
            if a.is_empty() || b.is_empty() {
                continue;
            }
            let key = if a <= b {
                format!("{a}||{b}")
            } else {
                format!("{b}||{a}")
            };
            *store.pairs.entry(key).or_insert(0) += count;
        }
        let _ = save_hub_cooccur(&state.cooccur_path, &store);
    }

    let store = load_hub_cooccur(&state.cooccur_path);
    let mut pairs: Vec<ModPairStat> = store
        .pairs
        .iter()
        .filter_map(|(k, &count)| {
            let mut parts = k.splitn(2, "||");
            Some(ModPairStat {
                mod_a: parts.next()?.to_string(),
                mod_b: parts.next()?.to_string(),
                count,
            })
        })
        .collect();
    pairs.sort_by(|a, b| b.count.cmp(&a.count));
    let limit = body
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(25)
        .min(100) as usize;
    pairs.truncate(limit);
    Json(json!({
        "mcVersion": store.mc_version,
        "loader": store.loader,
        "pairs": pairs,
    }))
}

fn load_hub_cooccur(path: &PathBuf) -> CooccurrenceStore {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_hub_cooccur(path: &PathBuf, store: &CooccurrenceStore) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        path,
        serde_json::to_vec_pretty(store).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
