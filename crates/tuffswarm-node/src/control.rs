//! Local control HTTP — same paths as tuffswarm-hub so the desktop bridge is uniform.
//! `/health` is open; `/v1/*` requires `Authorization: Bearer <control-token>`.

use axum::{
    extract::State,
    http::{header::AUTHORIZATION, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;
use tuffbox_core::crash_remote::{CrashLookupRequest, CrashLookupResponse};
use tuffbox_core::swarm::{CapsuleLibrary, ExperienceCapsule};

use crate::p2p::P2pHandle;

#[derive(Clone)]
struct AppState {
    library: Arc<Mutex<CapsuleLibrary>>,
    p2p: P2pHandle,
    control_token: Arc<String>,
}

pub async fn serve(
    addr: SocketAddr,
    library: Arc<Mutex<CapsuleLibrary>>,
    p2p: P2pHandle,
    control_token: String,
) -> anyhow::Result<()> {
    let state = AppState {
        library,
        p2p,
        control_token: Arc::new(control_token),
    };
    let protected = Router::new()
        .route("/v1/crash/capsules", post(publish_capsule))
        .route("/v1/crash/lookup", post(lookup))
        .route("/v1/node/status", get(node_status))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_bearer,
        ));

    let app = Router::new()
        .route("/health", get(health))
        .merge(protected)
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn require_bearer(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let expected = state.control_token.as_str();
    if expected.is_empty() {
        return Ok(next.run(req).await);
    }
    let header = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let token = header
        .strip_prefix("Bearer ")
        .or_else(|| header.strip_prefix("bearer "))
        .unwrap_or("");
    if token == expected {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Health {
    ok: bool,
    service: &'static str,
    capsule_count: usize,
    peers: usize,
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let count = state.library.lock().await.load_all().len();
    let peers = state.p2p.peer_count().await;
    Json(Health {
        ok: true,
        service: "tuffswarm-node",
        capsule_count: count,
        peers,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NodeStatus {
    peers: usize,
    listen_addrs: Vec<String>,
    capsule_count: usize,
}

async fn node_status(State(state): State<AppState>) -> impl IntoResponse {
    let capsule_count = state.library.lock().await.load_all().len();
    Json(NodeStatus {
        peers: state.p2p.peer_count().await,
        listen_addrs: state.p2p.listen_addrs().await,
        capsule_count,
    })
}

async fn publish_capsule(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<impl IntoResponse, (StatusCode, Json<Value>)> {
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
                "error": "raw logs are not accepted — ExperienceCapsule only"
            })),
        ));
    }
    if body.get("notes").is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "author notes must not be published" })),
        ));
    }

    let capsule = ExperienceCapsule::from_public_value(&body).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e })),
        )
    })?;

    let stored = {
        let lib = state.library.lock().await;
        lib.publish(&capsule).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e })),
            )
        })?
    };

    if let Err(e) = state.p2p.publish_capsule(stored.clone()).await {
        tracing::warn!(error = %e, "gossip publish failed (capsule still stored locally)");
    }

    Ok(Json(json!({
        "ok": true,
        "id": stored.id,
        "fingerprintKey": stored.fingerprint.key,
        "contentHash": stored.content_hash,
        "transport": "p2p+local",
    })))
}

async fn lookup(
    State(state): State<AppState>,
    Json(req): Json<CrashLookupRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<Value>)> {
    let hay = req.excerpt.clone().unwrap_or_default();
    let limit = req.limit.max(1).min(20) as usize;
    let hits = {
        let lib = state.library.lock().await;
        lib.lookup(&req.fingerprint, &hay, limit)
    };
    Ok(Json(CrashLookupResponse {
        kb_version: Some(format!(
            "p2p-{}",
            tuffbox_core::time_util::compact_now()
        )),
        hits,
    }))
}
