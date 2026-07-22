//! libp2p behaviour: Kademlia + Gossipsub + mDNS + Identify + AutoNAT + Relay + DCUtR.

use anyhow::Context;
use ed25519_dalek::SigningKey;
use futures::StreamExt;
use libp2p::gossipsub::{self, IdentTopic, MessageAuthenticity};
use libp2p::identity::Keypair;
use libp2p::kad::{store::MemoryStore, Behaviour as KadBehaviour, Event as KadEvent, Mode, RecordKey};
use libp2p::mdns;
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{autonat, dcutr, identify, relay, Multiaddr, PeerId, SwarmBuilder};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex, oneshot};
use tuffbox_core::swarm::{
    CapsuleLibrary, ExperienceCapsule, MAX_CAPSULE_GOSSIP_BYTES,
};

pub const CAPSULE_TOPIC: &str = "tuffswarm/capsules/v1";
pub const CAPABILITY_PREFIX: &str = "tuffswarm/cap/v1/";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeCapability {
    pub vram_mb: u32,
    pub rtt_ms: u32,
    pub version: String,
}

pub enum P2pCommand {
    PublishCapsule {
        capsule: ExperienceCapsule,
        reply: oneshot::Sender<Result<(), String>>,
    },
    PeerCount {
        reply: oneshot::Sender<usize>,
    },
    ListenAddrs {
        reply: oneshot::Sender<Vec<String>>,
    },
}

pub enum P2pEvent {
    CapsuleReceived(ExperienceCapsule),
    PeerCount(usize),
}

#[derive(Clone)]
pub struct P2pHandle {
    pub cmd_tx: mpsc::Sender<P2pCommand>,
}

impl P2pHandle {
    pub async fn publish_capsule(&self, capsule: ExperienceCapsule) -> Result<(), String> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(P2pCommand::PublishCapsule {
                capsule,
                reply: tx,
            })
            .await
            .map_err(|_| "p2p node command channel closed".to_string())?;
        rx.await
            .map_err(|_| "p2p node dropped publish reply".to_string())?
    }

    pub async fn peer_count(&self) -> usize {
        let (tx, rx) = oneshot::channel();
        if self.cmd_tx.send(P2pCommand::PeerCount { reply: tx }).await.is_err() {
            return 0;
        }
        rx.await.unwrap_or(0)
    }

    pub async fn listen_addrs(&self) -> Vec<String> {
        let (tx, rx) = oneshot::channel();
        if self
            .cmd_tx
            .send(P2pCommand::ListenAddrs { reply: tx })
            .await
            .is_err()
        {
            return Vec::new();
        }
        rx.await.unwrap_or_default()
    }
}

#[derive(NetworkBehaviour)]
struct SwarmBehaviour {
    gossipsub: gossipsub::Behaviour,
    kad: KadBehaviour<MemoryStore>,
    mdns: mdns::tokio::Behaviour,
    identify: identify::Behaviour,
    relay_client: relay::client::Behaviour,
    autonat: autonat::Behaviour,
    dcutr: dcutr::Behaviour,
    /// Present only when `--relay-server` is set (dummy otherwise via empty server).
    relay_server: relay::Behaviour,
}

fn signing_key_from_libp2p(id_keys: &Keypair) -> anyhow::Result<SigningKey> {
    let ed = id_keys
        .clone()
        .try_into_ed25519()
        .map_err(|_| anyhow::anyhow!("expected ed25519 identity key"))?;
    let secret: [u8; 32] = ed.secret().as_ref().try_into()?;
    Ok(SigningKey::from_bytes(&secret))
}

pub struct SwarmOpts {
    pub listen: String,
    pub bootstraps: Vec<String>,
    pub capability: NodeCapability,
    pub relay_server: bool,
}

pub async fn run_swarm(
    opts: SwarmOpts,
    library: Arc<Mutex<CapsuleLibrary>>,
    mut cmd_rx: mpsc::Receiver<P2pCommand>,
    event_tx: mpsc::Sender<P2pEvent>,
) -> anyhow::Result<()> {
    let id_keys = Keypair::generate_ed25519();
    let peer_id = PeerId::from(id_keys.public());
    let signing_key = signing_key_from_libp2p(&id_keys)?;
    let peer_id_str = peer_id.to_string();
    tracing::info!(%peer_id, relay_server = opts.relay_server, "local peer id");

    let message_id_fn = |message: &gossipsub::Message| {
        let mut hasher = DefaultHasher::new();
        message.data.hash(&mut hasher);
        gossipsub::MessageId::from(hasher.finish().to_string())
    };
    let gossipsub_config = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(Duration::from_secs(10))
        .validation_mode(gossipsub::ValidationMode::Permissive)
        .max_transmit_size(MAX_CAPSULE_GOSSIP_BYTES)
        .message_id_fn(message_id_fn)
        .build()
        .map_err(|e| anyhow::anyhow!("gossipsub config: {e}"))?;

    let mut gossipsub = gossipsub::Behaviour::new(
        MessageAuthenticity::Signed(id_keys.clone()),
        gossipsub_config,
    )
    .map_err(|e| anyhow::anyhow!("gossipsub: {e}"))?;
    let topic = IdentTopic::new(CAPSULE_TOPIC);
    gossipsub
        .subscribe(&topic)
        .map_err(|e| anyhow::anyhow!("subscribe: {e}"))?;

    let store = MemoryStore::new(peer_id);
    let mut kad = KadBehaviour::new(peer_id, store);
    kad.set_mode(Some(Mode::Server));

    let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)
        .context("mdns behaviour")?;
    let identify = identify::Behaviour::new(identify::Config::new(
        "/tuffswarm/1.0.0".into(),
        id_keys.public(),
    ));
    let autonat = autonat::Behaviour::new(peer_id, Default::default());
    let dcutr = dcutr::Behaviour::new(peer_id);
    let relay_server = if opts.relay_server {
        relay::Behaviour::new(peer_id, Default::default())
    } else {
        // Idle server behaviour (no reservations accepted usefully without listen config).
        relay::Behaviour::new(peer_id, Default::default())
    };

    let mut swarm = SwarmBuilder::with_existing_identity(id_keys)
        .with_tokio()
        .with_tcp(
            libp2p::tcp::Config::default(),
            libp2p::noise::Config::new,
            libp2p::yamux::Config::default,
        )?
        .with_relay_client(libp2p::noise::Config::new, libp2p::yamux::Config::default)?
        .with_behaviour(|_key, relay_client| {
            Ok(SwarmBehaviour {
                gossipsub,
                kad,
                mdns,
                identify,
                relay_client,
                autonat,
                dcutr,
                relay_server,
            })
        })?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    let listen_addr: Multiaddr = opts
        .listen
        .parse()
        .context("invalid --listen multiaddr")?;
    swarm.listen_on(listen_addr)?;
    if opts.relay_server {
        // Listen for relay circuit reservations on the same transport.
        tracing::info!("relay server mode enabled — publish this node's multiaddr as --bootstrap for NAT peers");
    }

    for boot in &opts.bootstraps {
        let addr: Multiaddr = boot
            .parse()
            .with_context(|| format!("invalid bootstrap multiaddr: {boot}"))?;
        let boot_peer = extract_peer_id(&addr).unwrap_or(peer_id);
        swarm.behaviour_mut().kad.add_address(&boot_peer, addr.clone());
        // Treat bootstrap as a potential relay.
        swarm.listen_on(addr.clone().with(libp2p::multiaddr::Protocol::P2pCircuit))?;
        if let Err(e) = swarm.dial(addr.clone()) {
            tracing::warn!(error = %e, %addr, "bootstrap dial failed");
        } else {
            tracing::info!(%addr, "dialing bootstrap");
        }
    }

    let cap_key = RecordKey::new(&format!("{CAPABILITY_PREFIX}{peer_id}"));
    let cap_bytes = serde_json::to_vec(&opts.capability)?;
    let record = libp2p::kad::Record {
        key: cap_key,
        value: cap_bytes,
        publisher: Some(peer_id),
        expires: None,
    };
    let _ = swarm
        .behaviour_mut()
        .kad
        .put_record(record, libp2p::kad::Quorum::One);

    // Seed gossip with recent local capsules (must be signed for P2P policy).
    {
        let lib = library.lock().await;
        for mut capsule in lib.load_all().into_iter().rev().take(16) {
            capsule = capsule.sanitized_for_network();
            if capsule.sign_ed25519(&signing_key, &peer_id_str).is_err() {
                continue;
            }
            let bytes = match serde_json::to_vec(&capsule.to_public_json()) {
                Ok(b) if b.len() <= MAX_CAPSULE_GOSSIP_BYTES => b,
                _ => continue,
            };
            let _ = swarm
                .behaviour_mut()
                .gossipsub
                .publish(topic.clone(), bytes);
        }
    }

    let mut connected: HashSet<PeerId> = HashSet::new();
    let mut listen_addrs: Vec<String> = Vec::new();

    loop {
        tokio::select! {
            cmd = cmd_rx.recv() => {
                let Some(cmd) = cmd else { break; };
                match cmd {
                    P2pCommand::PublishCapsule { capsule, reply } => {
                        let mut capsule = capsule.sanitized_for_network();
                        if let Err(e) = capsule.sign_ed25519(&signing_key, &peer_id_str) {
                            let _ = reply.send(Err(e));
                            continue;
                        }
                        let bytes = match serde_json::to_vec(&capsule.to_public_json()) {
                            Ok(b) => b,
                            Err(e) => {
                                let _ = reply.send(Err(e.to_string()));
                                continue;
                            }
                        };
                        if bytes.len() > MAX_CAPSULE_GOSSIP_BYTES {
                            let _ = reply.send(Err(format!(
                                "capsule exceeds max gossip size ({MAX_CAPSULE_GOSSIP_BYTES} bytes)"
                            )));
                            continue;
                        }
                        // Content-addressed DHT provider record.
                        let content_key = RecordKey::new(&capsule.dht_content_key());
                        let _ = swarm.behaviour_mut().kad.start_providing(content_key);
                        // Fingerprint provider for similarity discovery.
                        let fp_key = RecordKey::new(&format!(
                            "tuffswarm/fp/v1/{}",
                            capsule.fingerprint.key
                        ));
                        let _ = swarm.behaviour_mut().kad.start_providing(fp_key);

                        let result = swarm
                            .behaviour_mut()
                            .gossipsub
                            .publish(topic.clone(), bytes)
                            .map(|_| ())
                            .map_err(|e| format!("gossip publish: {e}"));
                        let _ = reply.send(result);
                    }
                    P2pCommand::PeerCount { reply } => {
                        let _ = reply.send(connected.len());
                    }
                    P2pCommand::ListenAddrs { reply } => {
                        let _ = reply.send(listen_addrs.clone());
                    }
                }
            }
            event = swarm.select_next_some() => {
                match event {
                    SwarmEvent::NewListenAddr { address, .. } => {
                        let full = format!("{address}/p2p/{peer_id}");
                        tracing::info!(%full, "listening");
                        listen_addrs.push(full);
                    }
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                        connected.insert(peer_id);
                        let _ = event_tx.send(P2pEvent::PeerCount(connected.len())).await;
                        swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                    }
                    SwarmEvent::ConnectionClosed { peer_id, .. } => {
                        connected.remove(&peer_id);
                        let _ = event_tx.send(P2pEvent::PeerCount(connected.len())).await;
                        swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                    }
                    SwarmEvent::Behaviour(SwarmBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                        for (peer, addr) in list {
                            tracing::info!(%peer, %addr, "mDNS discovered");
                            swarm.behaviour_mut().kad.add_address(&peer, addr);
                            swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer);
                        }
                    }
                    SwarmEvent::Behaviour(SwarmBehaviourEvent::Gossipsub(
                        gossipsub::Event::Message { message, .. }
                    )) => {
                        if message.data.len() > MAX_CAPSULE_GOSSIP_BYTES {
                            tracing::warn!("dropping oversized gossip message");
                            continue;
                        }
                        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&message.data) {
                            if let Ok(capsule) = ExperienceCapsule::from_public_value(&value) {
                                match capsule.accept_for_p2p_gossip() {
                                    Ok(()) => {
                                        let _ = event_tx.send(P2pEvent::CapsuleReceived(capsule)).await;
                                    }
                                    Err(e) => {
                                        tracing::debug!(error = %e, "dropped unsigned/invalid gossip capsule");
                                    }
                                }
                            }
                        }
                    }
                    SwarmEvent::Behaviour(SwarmBehaviourEvent::Kad(KadEvent::RoutingUpdated { peer, .. })) => {
                        tracing::debug!(%peer, "kad routing updated");
                    }
                    SwarmEvent::Behaviour(SwarmBehaviourEvent::Identify(identify::Event::Received {
                        peer_id,
                        info,
                        ..
                    })) => {
                        for addr in info.listen_addrs {
                            swarm.behaviour_mut().kad.add_address(&peer_id, addr);
                        }
                    }
                    SwarmEvent::Behaviour(SwarmBehaviourEvent::Autonat(ev)) => {
                        tracing::debug!(?ev, "autonat");
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn extract_peer_id(addr: &Multiaddr) -> Option<PeerId> {
    addr.iter().find_map(|p| match p {
        libp2p::multiaddr::Protocol::P2p(peer) => Some(peer),
        _ => None,
    })
}
