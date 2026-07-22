//! TuffSwarm Phase C node: libp2p (Kademlia + Gossipsub + AutoNAT + Relay) + local control HTTP.
//!
//! Same ExperienceCapsule schema as the HTTP hub. Desktop talks to `--control`
//! (127.0.0.1) for publish/lookup; P2P gossip distributes signed capsules to peers.
//! HTTP hub remains bootstrap/fallback at the launcher layer.
//!
//! Non-goals: Tenso/gRPC tensors, pipeline LLM, RepOps/Verde, LoRA training.

mod control;
mod p2p;

use anyhow::Context;
use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing_subscriber::EnvFilter;
use tuffbox_core::swarm::CapsuleLibrary;

use crate::p2p::{NodeCapability, P2pCommand, P2pHandle, SwarmOpts};

#[derive(Debug, Parser)]
#[command(name = "tuffswarm-node")]
#[command(about = "TuffSwarm Phase C P2P node (Kademlia + capsule gossip)")]
struct Args {
    /// libp2p listen multiaddr (default random high TCP port).
    #[arg(long, default_value = "/ip4/0.0.0.0/tcp/0")]
    listen: String,
    /// Local control HTTP for the launcher (publish/lookup/health).
    #[arg(long, default_value = "127.0.0.1:8790")]
    control: String,
    /// Bearer token for `/v1/*` (or set env `TUFFSWARM_CONTROL_TOKEN`).
    #[arg(long)]
    control_token: Option<String>,
    /// Optional bootstrap / relay multiaddr (`/ip4/.../tcp/.../p2p/...`).
    #[arg(long)]
    bootstrap: Vec<String>,
    /// Accept Circuit Relay v2 reservations (public hosts / hub co-location).
    #[arg(long, default_value_t = false)]
    relay_server: bool,
    /// Durable capsule JSONL directory.
    #[arg(long)]
    data_dir: Option<PathBuf>,
    /// Stub advertised VRAM (MB) for DHT capability record.
    #[arg(long, default_value_t = 0)]
    vram_mb: u32,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "tuffswarm_node=info,libp2p=warn".into()),
        )
        .init();

    let args = Args::parse();
    let data_dir = args.data_dir.unwrap_or_else(default_data_dir);
    std::fs::create_dir_all(&data_dir)?;
    let library = Arc::new(Mutex::new(CapsuleLibrary::open(
        data_dir.join("capsules.jsonl"),
    )));

    let control_token = args
        .control_token
        .filter(|t| !t.trim().is_empty())
        .or_else(|| {
            std::env::var("TUFFSWARM_CONTROL_TOKEN")
                .ok()
                .filter(|t| !t.trim().is_empty())
        })
        .unwrap_or_else(|| {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(
                format!(
                    "{}-{}",
                    std::process::id(),
                    tuffbox_core::time_util::compact_now()
                )
                .as_bytes(),
            );
            hex::encode(hasher.finalize())
        });

    let capability = NodeCapability {
        vram_mb: args.vram_mb,
        rtt_ms: 0,
        version: env!("CARGO_PKG_VERSION").to_string(),
    };

    let (cmd_tx, cmd_rx) = mpsc::channel::<P2pCommand>(64);
    let (event_tx, mut event_rx) = mpsc::channel(64);

    let opts = SwarmOpts {
        listen: args.listen.clone(),
        bootstraps: args.bootstrap.clone(),
        capability,
        relay_server: args.relay_server,
    };
    let lib_for_p2p = library.clone();
    tokio::spawn(async move {
        if let Err(e) = p2p::run_swarm(opts, lib_for_p2p, cmd_rx, event_tx).await {
            tracing::error!(error = %e, "p2p swarm stopped");
        }
    });

    // Persist gossiped capsules into the durable library.
    let lib_inbox = library.clone();
    tokio::spawn(async move {
        while let Some(ev) = event_rx.recv().await {
            match ev {
                p2p::P2pEvent::CapsuleReceived(capsule) => {
                    let lib = lib_inbox.lock().await;
                    match lib.publish(&capsule) {
                        Ok(stored) => {
                            tracing::info!(
                                id = %stored.id,
                                hash = ?stored.content_hash,
                                "stored gossip capsule"
                            );
                        }
                        Err(e) => tracing::warn!(error = %e, "reject gossip capsule"),
                    }
                }
                p2p::P2pEvent::PeerCount(n) => {
                    tracing::debug!(peers = n, "connected peers");
                }
            }
        }
    });

    let handle = P2pHandle { cmd_tx };
    let control_addr: SocketAddr = args
        .control
        .parse()
        .context("invalid --control listen address")?;
    tracing::info!(
        %control_addr,
        token_set = !control_token.is_empty(),
        "control HTTP (launcher bridge; /health open, /v1/* bearer)"
    );
    control::serve(control_addr, library, handle, control_token).await?;
    Ok(())
}

fn default_data_dir() -> PathBuf {
    dirs::data_dir()
        .or_else(dirs::config_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("TuffBox")
        .join("tuffswarm-node")
}
