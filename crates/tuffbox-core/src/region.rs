//! Anvil region (.mca) reader for 2D world map previews.
//!
//! Parses Minecraft region files (`saves/<world>/region/r.<x>.<z>.mca`)
//! according to the Anvil format:
//!   - 4096-byte location table  (5 bytes / chunk: 3-byte offset + 1-byte
//!     sector count + 1 unused byte)
//!   - 4096-byte timestamp table (4 bytes / chunk: last-modified epoch seconds)
//!   - chunk data sectors: 4-byte big-endian length + 1-byte compression type
//!     + compressed NBT
//!
//! This module powers a mcaselector-style 2D map: per-region chunk grids with
//! presence, last-modified time and a coarse generation status used for
//! coloring. It also supports deleting a set of chunks from region files,
//! which is the core mcaselector action (trim/clear selected chunks).

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};

/// A single chunk's summary inside a region file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkInfo {
    /// Block-X coordinate of the chunk (world space).
    pub x: i32,
    /// Block-Z coordinate of the chunk (world space).
    pub z: i32,
    /// Local chunk index (0..1024) within the region.
    pub index: usize,
    /// Whether the chunk has any data in the region file.
    pub present: bool,
    /// Last-modified epoch seconds (0 if absent).
    pub last_modified: u64,
    /// Coarse generation status, e.g. "empty", "structure_starts",
    /// "full", or unknown (""). Derived from the chunk root "Status" tag.
    pub status: String,
}

/// A region file and its 32x32 chunk grid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionInfo {
    /// Region coordinate X (file name r.<x>.<z>.mca).
    pub region_x: i32,
    /// Region coordinate Z.
    pub region_z: i32,
    /// Number of chunks present (offset != 0).
    pub present: usize,
    /// Earliest last-modified timestamp across present chunks.
    pub min_modified: u64,
    /// Latest last-modified timestamp across present chunks.
    pub max_modified: u64,
    /// Flat 1024-length grid (row-major, local chunk x + local z * 32),
    /// each entry a compact chunk summary for the 2D map.
    pub chunks: Vec<ChunkCell>,
}

/// Compact per-chunk cell for the 2D map (kept small for transfer).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkCell {
    /// 1 if the chunk has data, else 0.
    pub present: u8,
    /// Last-modified epoch seconds (0 if absent).
    pub last_modified: u64,
    /// Status code: 0 unknown, 1 empty, 2 partial, 3 full.
    pub status: u8,
}

/// Full map overview for a world.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldMap {
    /// Region coordinates found, for map bounds.
    pub regions: Vec<RegionInfo>,
    /// Min region X across all regions.
    pub min_region_x: i32,
    /// Min region Z.
    pub min_region_z: i32,
    /// Max region X (inclusive).
    pub max_region_x: i32,
    /// Max region Z (inclusive).
    pub max_region_z: i32,
    /// Total chunks present across all regions.
    pub total_present: usize,
    /// Total region files.
    pub region_count: usize,
}

/// Status codes shared with the frontend color mapping.
pub const STATUS_UNKNOWN: u8 = 0;
pub const STATUS_EMPTY: u8 = 1;
pub const STATUS_PARTIAL: u8 = 2;
pub const STATUS_FULL: u8 = 3;

/// Reads the 2D map overview for a world's region folder.
///
/// `world_dir` is the save directory containing `region/`. Returns an error if
/// the world or its region folder does not exist.
pub fn read_world_map(world_dir: &Path) -> Result<WorldMap, String> {
    let region_dir = world_dir.join("region");
    if !region_dir.is_dir() {
        return Err("no region folder (world not generated yet)".into());
    }

    let mut regions: Vec<RegionInfo> = Vec::new();
    let mut min_rx = i32::MAX;
    let mut min_rz = i32::MAX;
    let mut max_rx = i32::MIN;
    let mut max_rz = i32::MIN;
    let mut total_present = 0usize;

    let entries = std::fs::read_dir(&region_dir).map_err(|e| e.to_string())?;
    let mut names: Vec<PathBuf> = Vec::new();
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) == Some("mca") {
            names.push(p);
        }
    }
    names.sort();

    for path in names {
        let file_name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        // r.<rx>.<rz>.mca
        let parts: Vec<&str> = file_name.split('.').collect();
        if parts.len() < 3 || parts[0] != "r" {
            continue;
        }
        let rx: i32 = match parts[1].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let rz: i32 = match parts[2].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        match read_region(&path, rx, rz) {
            Ok(info) => {
                total_present += info.present;
                min_rx = min_rx.min(rx);
                min_rz = min_rz.min(rz);
                max_rx = max_rx.max(rx);
                max_rz = max_rz.max(rz);
                regions.push(info);
            }
            Err(_) => continue,
        }
    }

    if regions.is_empty() {
        return Err("no region files found".into());
    }

    regions.sort_by(|a, b| (a.region_z, a.region_x).cmp(&(b.region_z, b.region_x)));

    Ok(WorldMap {
        min_region_x: min_rx,
        min_region_z: min_rz,
        max_region_x: max_rx,
        max_region_z: max_rz,
        total_present,
        region_count: regions.len(),
        regions,
    })
}

/// Reads a single region file and returns its 32x32 chunk grid.
fn read_region(path: &Path, region_x: i32, region_z: i32) -> Result<RegionInfo, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    if data.len() < 8192 {
        return Err("region file too small".into());
    }

    let mut chunks = Vec::with_capacity(1024);
    let mut present_count = 0usize;
    let mut min_modified = u64::MAX;
    let mut max_modified = 0u64;

    for index in 0..1024 {
        let loc_off = index * 5;
        let offset = ((data[loc_off] as usize) << 16)
            | ((data[loc_off + 1] as usize) << 8)
            | (data[loc_off + 2] as usize);
        // sector count is data[loc_off + 3]; unused byte data[loc_off + 4].

        let ts_off = 4096 + index * 4;
        let last_modified = read_u32(&data, ts_off) as u64;

        let present = offset != 0;
        let mut status = STATUS_UNKNOWN;
        if present {
            present_count += 1;
            if last_modified != 0 {
                min_modified = min_modified.min(last_modified);
                max_modified = max_modified.max(last_modified);
            }
            // Read the chunk's coarse status from NBT to color the map like
            // mcaselector does.
            status = read_chunk_status(&data, offset);
        } else if last_modified != 0 {
            min_modified = min_modified.min(last_modified);
            max_modified = max_modified.max(last_modified);
        }

        chunks.push(ChunkCell {
            present: if present { 1 } else { 0 },
            last_modified,
            status,
        });
    }

    if min_modified == u64::MAX {
        min_modified = 0;
    }

    Ok(RegionInfo {
        region_x,
        region_z,
        present: present_count,
        min_modified,
        max_modified,
        chunks,
    })
}

/// Reads the coarse generation status of a chunk located at `offset`
/// (in sectors, each 4096 bytes) within the region `data`.
///
/// Returns a status code. Only the chunk's root "Status" string tag is
/// inspected — enough to separate empty / partial / full chunks for coloring.
fn read_chunk_status(data: &[u8], offset: usize) -> u8 {
    let base = offset * 4096;
    if base + 5 > data.len() {
        return STATUS_UNKNOWN;
    }
    let data_len = read_u32(data, base) as usize;
    let compression = data[base + 4];
    let payload_start = base + 5;
    if payload_start + data_len > data.len() || data_len == 0 {
        return STATUS_UNKNOWN;
    }
    let payload = &data[payload_start..payload_start + data_len];

    let decompressed = match decompress(compression, payload) {
        Some(d) => d,
        None => return STATUS_UNKNOWN,
    };

    status_from_nbt(&decompressed)
}

/// Maps a chunk's NBT to a coarse status code.
fn status_from_nbt(nbt: &[u8]) -> u8 {
    // Chunk root is a TAG_Compound (10). Find "Status" string tag.
    let mut pos = 1usize; // skip root type
    if pos >= nbt.len() || nbt[0] != 10 {
        return STATUS_UNKNOWN;
    }
    let name_len = read_u16(nbt, &mut pos) as usize;
    pos += name_len; // root compound name (usually empty)
    let status = find_string_in_compound(nbt, &mut pos, "Status");
    match status {
        Some(s) => classify_status(&s),
        None => STATUS_FULL,
    }
}

fn classify_status(s: &str) -> u8 {
    if s == "empty" || s.is_empty() {
        STATUS_EMPTY
    } else if s.contains("structure_starts")
        || s.contains("structure_references")
        || s.contains("biomes")
        || s.contains("noise")
        || s.contains("surface")
        || s.contains("heightmaps")
        || s.starts_with("minecraft:empty")
    {
        STATUS_PARTIAL
    } else {
        // "minecraft:full" and most others are considered fully generated
        STATUS_FULL
    }
}

/// Recursively scans a compound for a "Status" string tag.
fn find_string_in_compound(nbt: &[u8], pos: &mut usize, key: &str) -> Option<String> {
    while *pos < nbt.len() {
        let tag_type = nbt[*pos];
        *pos += 1;
        if tag_type == 0 {
            break; // TAG_End
        }
        let name_len = read_u16(nbt, pos) as usize;
        let name = String::from_utf8_lossy(&nbt[*pos..*pos + name_len]).to_string();
        *pos += name_len;

        if name == key && tag_type == 8 {
            // TAG_String
            let s_len = read_u16(nbt, pos) as usize;
            let s = String::from_utf8_lossy(&nbt[*pos..*pos + s_len]).to_string();
            *pos += s_len;
            return Some(s);
        }

        if tag_type == 10 {
            // Descend into nested compound and keep searching.
            if let Some(found) = find_string_in_compound(nbt, pos, key) {
                return Some(found);
            }
        } else {
            skip_payload(nbt, pos, tag_type);
        }
    }
    None
}

/// Skips the payload of a tag we don't care about while scanning for a key.
fn skip_payload(nbt: &[u8], pos: &mut usize, tag_type: u8) {
    match tag_type {
        1 => *pos += 1,
        2 => *pos += 2,
        3 => *pos += 4,
        4 => *pos += 8,
        5 => *pos += 4,
        6 => *pos += 8,
        7 => {
            // ByteArray
            let len = read_i32(nbt, pos) as usize;
            *pos += len;
        }
        8 => {
            // String
            let len = read_u16(nbt, pos) as usize;
            *pos += len;
        }
        9 => {
            // List
            let list_type = nbt[*pos];
            *pos += 1;
            let len = read_i32(nbt, pos) as usize;
            for _ in 0..len {
                if list_type == 10 {
                    // descend a compound and discard
                    let mut p = *pos;
                    let _ = find_string_in_compound(nbt, &mut p, "\0__never__");
                    *pos = p;
                } else {
                    skip_payload(nbt, pos, list_type);
                }
            }
        }
        10 => {
            // Compound: scan and discard
            let mut p = *pos;
            let _ = find_string_in_compound(nbt, &mut p, "\0__never__");
            *pos = p;
        }
        11 => {
            // IntArray
            let len = read_i32(nbt, pos) as usize;
            *pos += len * 4;
        }
        12 => {
            // LongArray
            let len = read_i32(nbt, pos) as usize;
            *pos += len * 8;
        }
        _ => {}
    }
}

/// Decompresses a chunk payload given its Anvil compression type.
fn decompress(compression: u8, payload: &[u8]) -> Option<Vec<u8>> {
    match compression {
        1 => {
            let mut decoder = flate2::read::GzDecoder::new(payload);
            let mut out = Vec::new();
            decoder.read_to_end(&mut out).ok()?;
            Some(out)
        }
        2 => {
            let mut decoder = flate2::read::ZlibDecoder::new(payload);
            let mut out = Vec::new();
            decoder.read_to_end(&mut out).ok()?;
            Some(out)
        }
        3 => Some(payload.to_vec()),
        4 => {
            // LZ4 (modern Minecraft) — use the lz4 crate if available.
            #[cfg(feature = "lz4")]
            {
                lz4::block::decompress(payload, None).ok()
            }
            #[cfg(not(feature = "lz4"))]
            {
                None
            }
        }
        _ => None,
    }
}

/// Deletes the given chunks from a world's region files.
///
/// `chunks` is a list of `r.<rx>.<rz>.mca` region coordinates paired with the
/// local chunk indices (0..1024) to clear. This mirrors mcaselector's core
/// action: selected chunks are removed from the region in place by zeroing
/// their location-table entry and timestamp.
///
/// Returns the number of chunk entries cleared.
pub fn delete_world_chunks(
    world_dir: &Path,
    selections: &[(i32, i32, Vec<usize>)],
) -> Result<usize, String> {
    let region_dir = world_dir.join("region");
    if !region_dir.is_dir() {
        return Err("no region folder".into());
    }

    let mut cleared = 0usize;

    for (rx, rz, indices) in selections {
        if indices.is_empty() {
            continue;
        }
        let file_name = format!("r.{}.{}.mca", rx, rz);
        let path = region_dir.join(&file_name);
        if !path.is_file() {
            continue;
        }
        let mut data = std::fs::read(&path).map_err(|e| e.to_string())?;
        if data.len() < 8192 {
            continue;
        }
        for &index in indices {
            if index >= 1024 {
                continue;
            }
            let loc_off = index * 5;
            // Zero the 4-byte offset + sector count (5th byte) and timestamp.
            for b in data.iter_mut().take(loc_off + 5).skip(loc_off) {
                *b = 0;
            }
            let ts_off = 4096 + index * 4;
            for b in data.iter_mut().take(ts_off + 4).skip(ts_off) {
                *b = 0;
            }
            cleared += 1;
        }
        std::fs::write(&path, &data).map_err(|e| e.to_string())?;
    }

    Ok(cleared)
}

fn read_u16(data: &[u8], pos: &mut usize) -> u16 {
    if *pos + 2 > data.len() {
        return 0;
    }
    let v = u16::from_be_bytes([data[*pos], data[*pos + 1]]);
    *pos += 2;
    v
}
fn read_u32(data: &[u8], pos: usize) -> u32 {
    if pos + 4 > data.len() {
        return 0;
    }
    u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
}
fn read_i32(data: &[u8], pos: &mut usize) -> i32 {
    if *pos + 4 > data.len() {
        return 0;
    }
    let v = i32::from_be_bytes([data[*pos], data[*pos + 1], data[*pos + 2], data[*pos + 3]]);
    *pos += 4;
    v
}
