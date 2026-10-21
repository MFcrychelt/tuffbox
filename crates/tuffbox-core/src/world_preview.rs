//! Section-level chunk data for the 3D world preview.
//!
//! [`crate::region`] owns Anvil I/O: region-file layout, decompression, chunk
//! read/write, caching, the 2D map. This module adds the one thing it does not
//! do — turn a chunk's *contents* into flat block-index arrays the mesher can
//! walk — and normalizes the two chunk layouts so the frontend never has to know
//! which Minecraft version wrote the world:
//!
//! - **1.13–1.17**: root → `Level` → `Sections[]` with `Palette` + `BlockStates`.
//! - **1.18+**: the chunk *is* the root → `sections[]` with
//!   `block_states { palette, data }` and `biomes { palette, data }`.
//!
//! Indices are unpacked **here**, not in the frontend, for two reasons: the
//! packing changed between versions (a continuous bit stream before 1.16,
//! long-aligned after), and packed arrays do not survive a JSON round trip —
//! a `i64` holding sixteen 4-bit indices exceeds 2^53 and would be silently
//! truncated by `JSON.parse` into a wrong world.

use crate::level_dat::{parse_nbt_binary, NbtTag};
use crate::region;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

/// Oldest DataVersion with the palette + packed-index format: 1.13 (1519).
pub const MIN_DATA_VERSION: i32 = 1519;
/// Newest DataVersion we will attempt to read: 26.2 (4903).
///
/// `ponytail:` the reader is layout-driven, not version-driven — it handles both
/// known section layouts and ignores extra fields. This bound exists so a
/// *future* layout change fails loudly instead of rendering garbage. Raise it
/// when a new layout ships and has been verified against a real world.
pub const READ_MAX_DATA_VERSION: i32 = 4903;

/// Blocks per section edge.
pub const SECTION_SIZE: usize = 16;
/// Block entries in one section.
pub const SECTION_BLOCKS: usize = SECTION_SIZE * SECTION_SIZE * SECTION_SIZE;
/// Biome entries in one section (4×4×4).
pub const SECTION_BIOMES: usize = 4 * 4 * 4;

/// One block-state entry of a section palette.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaletteEntry {
    pub name: String,
    /// Block properties (`axis=y`, `half=top`, …), sorted for stable output.
    pub properties: BTreeMap<String, String>,
}

/// One 16×16×16 section, normalized across layouts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionColumn {
    /// Section index; block Y = `y * 16`.
    pub y: i8,
    pub block_palette: Vec<PaletteEntry>,
    /// [`SECTION_BLOCKS`] palette indices as base64 little-endian `u16`, in
    /// vanilla order `index = (y << 8) | (z << 4) | x`.
    pub block_states: String,
    pub biome_palette: Vec<String>,
    /// [`SECTION_BIOMES`] palette indices, same encoding as `block_states`.
    /// Empty on worlds older than 1.18, which store biomes per chunk instead.
    pub biome_states: String,
}

/// One chunk column, ready to hand to the renderer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChunkColumn {
    pub x: i32,
    pub z: i32,
    pub data_version: i32,
    /// Vanilla chunk status (`minecraft:full`, `full`, …). Prototype chunks are
    /// not worth meshing; the caller decides whether to ask for them.
    pub status: Option<String>,
    pub sections: Vec<SectionColumn>,
}

fn compound(tag: &NbtTag) -> Option<&[(String, NbtTag)]> {
    match tag {
        NbtTag::Compound(entries) => Some(entries),
        _ => None,
    }
}

fn field<'a>(entries: &'a [(String, NbtTag)], key: &str) -> Option<&'a NbtTag> {
    entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn as_i32(tag: &NbtTag) -> Option<i32> {
    match tag {
        NbtTag::Int(v) => Some(*v),
        NbtTag::Short(v) => Some(*v as i32),
        NbtTag::Byte(v) => Some(*v as i32),
        _ => None,
    }
}

fn as_string(tag: &NbtTag) -> Option<String> {
    match tag {
        NbtTag::String(s) => Some(s.clone()),
        _ => None,
    }
}

fn as_long_array(tag: &NbtTag) -> Option<&[i64]> {
    match tag {
        NbtTag::LongArray(v) => Some(v),
        _ => None,
    }
}

fn as_list(tag: &NbtTag) -> Option<&[NbtTag]> {
    match tag {
        NbtTag::List(items) => Some(items),
        _ => None,
    }
}

/// `{ Name: "minecraft:stone", Properties: { … } }` → [`PaletteEntry`].
fn palette_entry(tag: &NbtTag) -> Option<PaletteEntry> {
    let entries = compound(tag)?;
    let name = field(entries, "Name").and_then(as_string)?;
    let properties = field(entries, "Properties")
        .and_then(compound)
        .map(|props| {
            props
                .iter()
                .filter_map(|(k, v)| as_string(v).map(|value| (k.clone(), value)))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    Some(PaletteEntry { name, properties })
}

/// Bits per index for a palette of `len` entries, floored at `min_bits`
/// (vanilla floors block states at 4 and biomes at 1).
///
/// A palette with at most one entry has no `data` array on disk at all: every
/// index is 0. Returning 0 bits is how that case travels through the decoder.
fn bits_for(len: usize, min_bits: u32) -> u32 {
    if len <= 1 {
        return 0;
    }
    let needed = (usize::BITS - (len - 1).leading_zeros()).max(min_bits);
    needed.min(32)
}

/// Unpack palette indices, choosing the packing scheme from the array length.
///
/// When `bits` divides 64 the two schemes produce the same lengths *and* the
/// same values (an index that starts at a nibble/byte boundary never straddles
/// a long), so preferring the padded branch on ambiguity cannot change the
/// result — it only matters for `bits` that do not divide 64, where the lengths
/// differ and the choice is unambiguous.
fn unpack_indices(data: &[i64], bits: u32, count: usize) -> Result<Vec<u16>, String> {
    // 0 bits = palette of one: every entry is that block, and vanilla omits the
    // `data` array entirely.
    if bits == 0 {
        return Ok(vec![0; count]);
    }
    if bits > 32 {
        return Err(format!("implausible palette width {bits}"));
    }
    let per_long = (64 / bits) as usize;
    let padded_len = count.div_ceil(per_long);
    let stream_len = (count * bits as usize).div_ceil(64);
    let padded = data.len() == padded_len || data.len() != stream_len;
    let needed = if padded { padded_len } else { stream_len };
    if data.len() < needed {
        return Err(format!(
            "palette data is short: {} longs, need {needed} (bits {bits}, {count} entries)",
            data.len()
        ));
    }

    let mask = (1u64 << bits) - 1;
    let mut out = Vec::with_capacity(count);
    if padded {
        for index in 0..count {
            let word = data[index / per_long] as u64;
            let shift = ((index % per_long) as u32) * bits;
            out.push(((word >> shift) & mask) as u16);
        }
    } else {
        for index in 0..count {
            let start = index as u64 * bits as u64;
            let long_index = (start / 64) as usize;
            let offset = (start % 64) as u32;
            let mut word = (data[long_index] as u64) >> offset;
            let taken = 64 - offset;
            if taken < bits {
                // The index continues in the next long (1.13–1.15 layout).
                word |= (data[long_index + 1] as u64) << taken;
            }
            out.push((word & mask) as u16);
        }
    }
    Ok(out)
}

fn encode_indices(indices: &[u16]) -> String {
    let mut bytes = Vec::with_capacity(indices.len() * 2);
    for value in indices {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    STANDARD.encode(bytes)
}

/// Normalize a parsed chunk NBT into a [`ChunkColumn`].
pub fn chunk_column(tag: &NbtTag) -> Result<ChunkColumn, String> {
    let root = compound(tag).ok_or("chunk root is not a compound")?;

    // 1.17 and older wrap the chunk fields in `Level`; 1.18+ put them at the root.
    let level = field(root, "Level").and_then(compound).unwrap_or(root);

    let data_version = field(level, "DataVersion")
        .or_else(|| field(root, "DataVersion"))
        .and_then(as_i32)
        .unwrap_or(0);
    if data_version != 0 && !(MIN_DATA_VERSION..=READ_MAX_DATA_VERSION).contains(&data_version) {
        return Err(format!(
            "unsupported chunk DataVersion {data_version} (reader supports {MIN_DATA_VERSION}..={READ_MAX_DATA_VERSION})"
        ));
    }

    let x = field(level, "xPos")
        .or_else(|| field(root, "xPos"))
        .and_then(as_i32)
        .unwrap_or(0);
    let z = field(level, "zPos")
        .or_else(|| field(root, "zPos"))
        .and_then(as_i32)
        .unwrap_or(0);
    let status = field(level, "Status")
        .or_else(|| field(root, "Status"))
        .and_then(as_string);

    // Modern is `sections`, legacy is `Sections`.
    let mut sections: Vec<SectionColumn> = field(level, "sections")
        .or_else(|| field(level, "Sections"))
        .and_then(as_list)
        .map(|items| items.iter().filter_map(section_column).collect())
        .unwrap_or_default();
    sections.sort_by_key(|section| section.y);

    Ok(ChunkColumn {
        x,
        z,
        data_version,
        status,
        sections,
    })
}

/// A section that cannot be decoded is dropped rather than failing the chunk —
/// one odd section should not cost the player the whole column.
fn section_column(tag: &NbtTag) -> Option<SectionColumn> {
    let entries = compound(tag)?;
    let y = match field(entries, "Y") {
        Some(NbtTag::Byte(v)) => *v,
        Some(other) => i8::try_from(as_i32(other)?).ok()?,
        None => 0,
    };

    let block_states = field(entries, "block_states").and_then(compound);
    let (block_palette, block_data) = match block_states {
        // 1.18+ nests the palette under `block_states`.
        Some(states) => (
            palette_list(field(states, "palette")),
            field(states, "data").and_then(as_long_array).unwrap_or(&[]),
        ),
        // 1.13–1.17 keep `Palette` + `BlockStates` directly on the section.
        None => (
            palette_list(field(entries, "Palette")),
            field(entries, "BlockStates")
                .and_then(as_long_array)
                .unwrap_or(&[]),
        ),
    };

    let bits = bits_for(block_palette.len(), 4);
    let block_indices = unpack_indices(block_data, bits, SECTION_BLOCKS).ok()?;

    let (biome_palette, biome_states) = match field(entries, "biomes").and_then(compound) {
        Some(biomes) => {
            let palette: Vec<String> = field(biomes, "palette")
                .and_then(as_list)
                .map(|items| items.iter().filter_map(as_string).collect())
                .unwrap_or_default();
            let data = field(biomes, "data").and_then(as_long_array).unwrap_or(&[]);
            let indices =
                unpack_indices(data, bits_for(palette.len(), 1), SECTION_BIOMES).ok()?;
            (palette, encode_indices(&indices))
        }
        // Pre-1.18 keeps one chunk-scoped 1024-entry biome array on the root, so
        // there is nothing section-scoped to expose. The renderer falls back to
        // a default biome for those versions.
        None => (Vec::new(), String::new()),
    };

    Some(SectionColumn {
        y,
        block_palette,
        block_states: encode_indices(&block_indices),
        biome_palette,
        biome_states,
    })
}

fn palette_list(tag: Option<&NbtTag>) -> Vec<PaletteEntry> {
    tag.and_then(as_list)
        .map(|items| items.iter().filter_map(palette_entry).collect())
        .unwrap_or_default()
}

/// Reads and normalizes the chunk at global chunk coordinates `(chunk_x, chunk_z)`.
///
/// `Ok(None)` means the world simply has no chunk there (nothing generated) —
/// not an error the caller should report to the user.
pub fn load_column(
    world_dir: &Path,
    dimension: Option<&str>,
    chunk_x: i32,
    chunk_z: i32,
) -> Result<Option<ChunkColumn>, String> {
    let region_x = chunk_x >> 5;
    let region_z = chunk_z >> 5;
    // `& 31` is the positive remainder for negative coordinates too, which is
    // exactly the Anvil index convention.
    let index = ((chunk_z & 31) * 32 + (chunk_x & 31)) as usize;
    let region_dir =
        region::dimension_region_dir(world_dir, region::normalize_dimension(dimension));

    let Some((blob, _timestamp)) = region::load_chunk_raw(&region_dir, region_x, region_z, index)?
    else {
        return Ok(None);
    };
    let nbt = region::decompress_chunk_blob(&blob)
        .ok_or_else(|| format!("chunk {chunk_x},{chunk_z} uses an unsupported compression type"))?;
    let tag = parse_nbt_binary(&nbt).map_err(|e| format!("chunk {chunk_x},{chunk_z} NBT: {e}"))?;
    chunk_column(&tag).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal NBT writer. The reader is what is under test, so fixtures are
    /// built by hand instead of round-tripping through a second parser.
    mod nbt_write {
        pub fn tag(out: &mut Vec<u8>, kind: u8, name: &str) {
            out.push(kind);
            str_payload(out, name);
        }

        pub fn str_payload(out: &mut Vec<u8>, s: &str) {
            out.extend_from_slice(&(s.len() as u16).to_be_bytes());
            out.extend_from_slice(s.as_bytes());
        }

        pub fn i32_payload(out: &mut Vec<u8>, v: i32) {
            out.extend_from_slice(&v.to_be_bytes());
        }

        pub fn long_array(out: &mut Vec<u8>, name: &str, values: &[i64]) {
            tag(out, 12, name);
            i32_payload(out, values.len() as i32);
            for v in values {
                out.extend_from_slice(&v.to_be_bytes());
            }
        }

        pub fn end_compound(out: &mut Vec<u8>) {
            out.push(0);
        }
    }

    fn decode_u16(encoded: &str) -> Vec<u16> {
        let bytes = STANDARD.decode(encoded).expect("base64");
        bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect()
    }

    /// One 1.18-style section whose block palette has `palette_len` entries and
    /// whose `data` array is `data`.
    fn modern_chunk_nbt(palette_len: usize, data: &[i64]) -> Vec<u8> {
        use nbt_write::*;
        let mut out = Vec::new();
        out.push(10); // root compound
        str_payload(&mut out, "");
        tag(&mut out, 3, "DataVersion");
        i32_payload(&mut out, 3465); // 1.20.1
        tag(&mut out, 3, "xPos");
        i32_payload(&mut out, 7);
        tag(&mut out, 3, "zPos");
        i32_payload(&mut out, -3);
        tag(&mut out, 8, "Status");
        str_payload(&mut out, "minecraft:full");

        tag(&mut out, 9, "sections");
        out.push(10); // list of compounds
        i32_payload(&mut out, 1);
        tag(&mut out, 1, "Y");
        out.push(4u8); // y = 4

        tag(&mut out, 10, "block_states");
        tag(&mut out, 9, "palette");
        out.push(10); // palette of compounds
        i32_payload(&mut out, palette_len as i32);
        for index in 0..palette_len {
            tag(&mut out, 8, "Name");
            str_payload(&mut out, &format!("minecraft:block{index}"));
            tag(&mut out, 10, "Properties");
            end_compound(&mut out); // empty Properties
            end_compound(&mut out); // end entry
        }
        long_array(&mut out, "data", data);
        end_compound(&mut out); // end block_states

        tag(&mut out, 10, "biomes");
        tag(&mut out, 9, "palette");
        out.push(8); // palette of strings
        i32_payload(&mut out, 1);
        str_payload(&mut out, "minecraft:plains");
        end_compound(&mut out); // end biomes

        end_compound(&mut out); // end section
        end_compound(&mut out); // end sections
        end_compound(&mut out); // end root
        out
    }

    /// A 1.15-style chunk: `Level` → `Sections[]` with `Palette`/`BlockStates`.
    fn legacy_chunk_nbt() -> Vec<u8> {
        use nbt_write::*;
        let mut out = Vec::new();
        out.push(10); // root compound
        str_payload(&mut out, "");
        tag(&mut out, 3, "DataVersion");
        i32_payload(&mut out, 2230); // 1.15.2

        tag(&mut out, 10, "Level");
        tag(&mut out, 3, "xPos");
        i32_payload(&mut out, -32);
        tag(&mut out, 3, "zPos");
        i32_payload(&mut out, 64);
        tag(&mut out, 8, "Status");
        str_payload(&mut out, "full");

        tag(&mut out, 9, "Sections");
        out.push(10);
        i32_payload(&mut out, 1);
        tag(&mut out, 1, "Y");
        out.push(0u8);
        tag(&mut out, 9, "Palette");
        out.push(10);
        i32_payload(&mut out, 1);
        tag(&mut out, 8, "Name");
        str_payload(&mut out, "minecraft:bedrock");
        end_compound(&mut out); // end palette entry
        long_array(&mut out, "BlockStates", &[7; 256]);
        end_compound(&mut out); // end section
        end_compound(&mut out); // end Sections
        end_compound(&mut out); // end Level
        end_compound(&mut out); // end root
        out
    }

    /// Parks a chunk in a temp world through the real writer, so the test
    /// exercises region I/O and this module together.
    fn world_with_chunk(nbt: &[u8], chunk_x: i32, chunk_z: i32) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let region_dir = dir.path().join("region");
        std::fs::create_dir_all(&region_dir).expect("mkdir");
        let blob = region::compress_chunk_blob(2, nbt).expect("compress");
        let index = ((chunk_z & 31) * 32 + (chunk_x & 31)) as usize;
        region::save_chunk_raw(&region_dir, chunk_x >> 5, chunk_z >> 5, index, &blob, 1)
            .expect("save");
        dir
    }

    #[test]
    fn decodes_aligned_indices() {
        // 4 bits per index: sixteen of them share one long, lowest nibble first.
        let data = [0x3210i64, 5];
        let indices = unpack_indices(&data, 4, 17).unwrap();
        assert_eq!(&indices[..4], &[0, 1, 2, 3]);
        assert_eq!(&indices[4..16], &[0u16; 12]);
        assert_eq!(indices[16], 5, "index 16 is the first nibble of the second long");
    }

    #[test]
    fn a_palette_with_one_entry_is_all_zero() {
        assert_eq!(bits_for(1, 4), 0);
        assert_eq!(unpack_indices(&[], 0, 4).unwrap(), vec![0, 0, 0, 0]);
    }

    #[test]
    fn decodes_indices_that_straddle_longs() {
        // 5 bits does not divide 64: the pre-1.16 layout is used, and an index
        // crossing a long boundary must still decode. Pack `count` indices into
        // the continuous stream by hand, then read them back.
        const BITS: u32 = 5;
        const COUNT: usize = 64;
        let mut longs = vec![0i64; (COUNT * BITS as usize).div_ceil(64)];
        for (index, value) in (0..COUNT).map(|i| (i, (i as u16 * 7) & 0x1F)) {
            let start = index as u64 * BITS as u64;
            let long = (start / 64) as usize;
            let offset = (start % 64) as u32;
            let mut word = longs[long] as u64;
            word |= (value as u64) << offset;
            longs[long] = word as i64;
            if offset + BITS > 64 {
                let spill = 64 - offset;
                let mut next = longs[long + 1] as u64;
                next |= (value as u64) >> spill;
                longs[long + 1] = next as i64;
            }
        }
        let decoded = unpack_indices(&longs, BITS, COUNT).unwrap();
        for (index, value) in decoded.iter().enumerate() {
            assert_eq!(*value, (index as u16 * 7) & 0x1F, "index {index}");
        }
    }

    #[test]
    fn a_short_palette_array_is_an_error_not_a_panic() {
        assert!(unpack_indices(&[1, 2], 4, SECTION_BLOCKS).is_err());
    }

    #[test]
    fn reads_the_modern_layout() {
        let world = world_with_chunk(&modern_chunk_nbt(2, &[0x1111_1111_1111_1111u64 as i64; 256]), 7, -3);
        let column = load_column(world.path(), None, 7, -3)
            .expect("load")
            .expect("chunk present");

        assert_eq!((column.x, column.z), (7, -3));
        assert_eq!(column.data_version, 3465);
        assert_eq!(column.status.as_deref(), Some("minecraft:full"));
        assert_eq!(column.sections.len(), 1);

        let section = &column.sections[0];
        assert_eq!(section.y, 4);
        assert_eq!(section.block_palette.len(), 2);
        assert_eq!(section.block_palette[0].name, "minecraft:block0");
        assert_eq!(section.block_palette[1].name, "minecraft:block1");
        let states = decode_u16(&section.block_states);
        assert_eq!(states.len(), SECTION_BLOCKS);
        assert!(states.iter().all(|index| *index == 1));

        assert_eq!(section.biome_palette, vec!["minecraft:plains".to_string()]);
        let biomes = decode_u16(&section.biome_states);
        assert_eq!(biomes.len(), SECTION_BIOMES);
        assert!(biomes.iter().all(|index| *index == 0));
    }

    #[test]
    fn reads_the_pre_1_18_layout_into_the_same_shape() {
        let world = world_with_chunk(&legacy_chunk_nbt(), -32, 64);
        let column = load_column(world.path(), None, -32, 64)
            .expect("load")
            .expect("chunk present");

        assert_eq!(column.data_version, 2230);
        assert_eq!((column.x, column.z), (-32, 64));
        assert_eq!(column.status.as_deref(), Some("full"));
        assert_eq!(column.sections.len(), 1);

        let section = &column.sections[0];
        assert_eq!(section.y, 0);
        assert_eq!(section.block_palette[0].name, "minecraft:bedrock");
        // A one-entry palette means every block in the section *is* that entry,
        // so the decoder reports index 0 for all 4096 positions.
        let states = decode_u16(&section.block_states);
        assert_eq!(states.len(), SECTION_BLOCKS);
        assert!(states.iter().all(|index| *index == 0));
        // Pre-1.18 biomes are chunk-scoped, so no per-section palette is exposed.
        assert!(section.biome_palette.is_empty());
        assert!(section.biome_states.is_empty());
    }

    #[test]
    fn a_missing_chunk_is_none_not_an_error() {
        let world = world_with_chunk(&modern_chunk_nbt(2, &[0; 256]), 7, -3);
        assert!(load_column(world.path(), None, 7, -4).unwrap().is_none());
        assert!(load_column(world.path(), None, 900, 900).unwrap().is_none());
    }

    #[test]
    fn rejects_a_version_the_reader_does_not_know() {
        let tag = NbtTag::Compound(vec![
            ("DataVersion".into(), NbtTag::Int(99)),
            ("sections".into(), NbtTag::List(vec![])),
        ]);
        let error = chunk_column(&tag).unwrap_err();
        assert!(error.contains("unsupported chunk DataVersion 99"), "{error}");
    }

    #[test]
    fn a_chunk_header_that_overflows_the_file_is_treated_as_absent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let region_dir = dir.path().join("region");
        std::fs::create_dir_all(&region_dir).expect("mkdir");
        // Declares a payload far longer than the file can hold: the region
        // layer must reject it as "no such chunk", not slice out of bounds.
        let mut blob = Vec::new();
        blob.extend_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
        blob.push(2);
        blob.extend_from_slice(b"not really nbt");
        region::save_chunk_raw(&region_dir, 0, 0, 0, &blob, 1).expect("save");
        assert!(load_column(dir.path(), None, 0, 0).unwrap().is_none());
    }

    #[test]
    fn a_chunk_whose_payload_is_not_nbt_reports_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let region_dir = dir.path().join("region");
        std::fs::create_dir_all(&region_dir).expect("mkdir");
        let blob = region::compress_chunk_blob(3, b"this is not NBT").expect("compress");
        region::save_chunk_raw(&region_dir, 0, 0, 0, &blob, 1).expect("save");
        let error = load_column(dir.path(), None, 0, 0).unwrap_err();
        assert!(error.contains("NBT"), "unexpected error: {error}");
    }
}
