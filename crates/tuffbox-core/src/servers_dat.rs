//! Read Minecraft `servers.dat` (uncompressed or gzip NBT).
//!
//! Format: root compound with a `servers` list of compounds
//! (`name`, `ip`, optional `icon`, `acceptTextures`).

use crate::level_dat::{self, NbtTag};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerEntry {
    pub name: String,
    pub address: String,
    pub icon: Option<String>,
    pub accept_textures: Option<i8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerPingResult {
    pub address: String,
    pub online: bool,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
}

/// Lists servers from `project_dir/servers.dat`. Empty vec if missing.
pub fn list_servers(project_dir: &Path) -> Result<Vec<ServerEntry>, String> {
    let path = project_dir.join("servers.dat");
    if !path.is_file() {
        return Ok(vec![]);
    }
    let tag = level_dat::read_nbt_auto(&path)?;
    Ok(extract_servers(&tag))
}

fn extract_servers(tag: &NbtTag) -> Vec<ServerEntry> {
    let entries = match tag {
        NbtTag::Compound(e) => e,
        _ => return vec![],
    };
    let list = entries
        .iter()
        .find(|(k, _)| k == "servers")
        .and_then(|(_, v)| match v {
            NbtTag::List(items) => Some(items),
            _ => None,
        });
    let Some(items) = list else {
        return vec![];
    };

    let mut out = Vec::new();
    for item in items {
        let NbtTag::Compound(fields) = item else {
            continue;
        };
        let name = get_string(fields, "name").unwrap_or_else(|| "Minecraft Server".into());
        let address = get_string(fields, "ip").unwrap_or_default();
        if address.is_empty() {
            continue;
        }
        let icon = get_string(fields, "icon");
        let accept_textures = get_byte(fields, "acceptTextures");
        out.push(ServerEntry {
            name,
            address,
            icon,
            accept_textures,
        });
    }
    out
}

fn get_string(fields: &[(String, NbtTag)], key: &str) -> Option<String> {
    fields.iter().find(|(k, _)| k == key).and_then(|(_, v)| match v {
        NbtTag::String(s) => Some(s.clone()),
        _ => None,
    })
}

fn get_byte(fields: &[(String, NbtTag)], key: &str) -> Option<i8> {
    fields.iter().find(|(k, _)| k == key).and_then(|(_, v)| match v {
        NbtTag::Byte(b) => Some(*b),
        _ => None,
    })
}

/// TCP connect latency probe (not full Minecraft handshake).
pub fn ping_server_address(address: &str) -> ServerPingResult {
    let addr = address.trim();
    if addr.is_empty() {
        return ServerPingResult {
            address: address.to_string(),
            online: false,
            latency_ms: None,
            error: Some("empty address".into()),
        };
    }

    let with_port = if addr.contains(':') {
        addr.to_string()
    } else {
        format!("{addr}:25565")
    };

    let start = Instant::now();
    match with_port.to_socket_addrs() {
        Ok(mut iter) => {
            let Some(sock): Option<SocketAddr> = iter.next() else {
                return ServerPingResult {
                    address: address.to_string(),
                    online: false,
                    latency_ms: None,
                    error: Some("could not resolve host".into()),
                };
            };
            match TcpStream::connect_timeout(&sock, Duration::from_secs(3)) {
                Ok(_) => ServerPingResult {
                    address: address.to_string(),
                    online: true,
                    latency_ms: Some(start.elapsed().as_millis() as u64),
                    error: None,
                },
                Err(e) => ServerPingResult {
                    address: address.to_string(),
                    online: false,
                    latency_ms: None,
                    error: Some(e.to_string()),
                },
            }
        }
        Err(e) => ServerPingResult {
            address: address.to_string(),
            online: false,
            latency_ms: None,
            error: Some(e.to_string()),
        },
    }
}

/// Append a server to servers.dat (creates file if missing).
pub fn add_server(project_dir: &Path, name: &str, address: &str) -> Result<Vec<ServerEntry>, String> {
    let name = name.trim();
    let address = address.trim();
    if name.is_empty() || address.is_empty() {
        return Err("name and address required".into());
    }
    let mut servers = list_servers(project_dir)?;
    if servers.iter().any(|s| s.address.eq_ignore_ascii_case(address)) {
        return Err("server already exists".into());
    }
    servers.push(ServerEntry {
        name: name.to_string(),
        address: address.to_string(),
        icon: None,
        accept_textures: None,
    });
    write_servers(project_dir, &servers)?;
    Ok(servers)
}

/// Remove server by address (case-insensitive).
pub fn remove_server(project_dir: &Path, address: &str) -> Result<Vec<ServerEntry>, String> {
    let mut servers = list_servers(project_dir)?;
    let before = servers.len();
    servers.retain(|s| !s.address.eq_ignore_ascii_case(address));
    if servers.len() == before {
        return Err("server not found".into());
    }
    write_servers(project_dir, &servers)?;
    Ok(servers)
}

fn write_servers(project_dir: &Path, servers: &[ServerEntry]) -> Result<(), String> {
    let path = project_dir.join("servers.dat");
    let bytes = encode_servers_nbt(servers)?;
    let mut f = fs::File::create(&path).map_err(|e| e.to_string())?;
    f.write_all(&bytes).map_err(|e| e.to_string())?;
    Ok(())
}

fn encode_servers_nbt(servers: &[ServerEntry]) -> Result<Vec<u8>, String> {
    // Uncompressed root compound named "" with list "servers".
    let mut buf = Vec::new();
    buf.push(10); // TAG_Compound
    write_string(&mut buf, "");
    // TAG_List "servers"
    buf.push(9);
    write_string(&mut buf, "servers");
    buf.push(10); // list of compounds
    buf.extend_from_slice(&(servers.len() as i32).to_be_bytes());
    for s in servers {
        // compound fields
        buf.push(8); // string name
        write_string(&mut buf, "name");
        write_string(&mut buf, &s.name);
        buf.push(8); // string ip
        write_string(&mut buf, "ip");
        write_string(&mut buf, &s.address);
        if let Some(ref icon) = s.icon {
            buf.push(8);
            write_string(&mut buf, "icon");
            write_string(&mut buf, icon);
        }
        if let Some(at) = s.accept_textures {
            buf.push(1);
            write_string(&mut buf, "acceptTextures");
            buf.push(at as u8);
        }
        buf.push(0); // TAG_End of compound
    }
    buf.push(0); // TAG_End of root
    Ok(buf)
}

fn write_string(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    buf.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
    buf.extend_from_slice(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_servers_dat() {
        let dir = std::env::temp_dir().join(format!("tuffbox_srv_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        add_server(&dir, "Local", "127.0.0.1:25565").unwrap();
        add_server(&dir, "Hypixel", "mc.hypixel.net").unwrap();
        let listed = list_servers(&dir).unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].name, "Local");
        remove_server(&dir, "127.0.0.1:25565").unwrap();
        assert_eq!(list_servers(&dir).unwrap().len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }
}
