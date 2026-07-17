//! Murmur2 hash used by CurseForge for file fingerprinting.
//!
//! CurseForge's `packageFingerprint` (used in the `fingerprint` API and to
//! validate files in a CF modpack export) is a MurmurHash2 over the file
//! bytes with seed `1`. This mirrors packwiz's `curseforge/murmur2/hash.go`.
//! TuffBox needs it when producing CurseForge-compatible exports where the
//! recipient launcher may verify fingerprints.

use std::io::Read;

const SEED: u32 = 1;
const M: u32 = 0x5bd1_e995;

/// Computes the CurseForge MurmurHash2 (32-bit, little-endian, seed = 1) of
/// the given reader's contents.
pub fn murmur2<R: Read>(mut reader: R) -> u32 {
    const R: u32 = 24;
    let mut buf = [0u8; 4096];
    let mut total_len: u32 = 0;
    // h1 = seed ^ len
    let mut hash: u32 = SEED;
    let mut blocks: Vec<u8> = Vec::new();
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        total_len = total_len.wrapping_add(n as u32);
        blocks.extend_from_slice(&buf[..n]);
    }

    let len = blocks.len();
    hash ^= len as u32;

    let mut i = 0;
    while i + 4 <= len {
        let mut k = u32::from_le_bytes([blocks[i], blocks[i + 1], blocks[i + 2], blocks[i + 3]]);
        i += 4;
        k = k.wrapping_mul(M);
        k ^= k >> R;
        k = k.wrapping_mul(M);
        hash = hash.wrapping_mul(M);
        hash ^= k;
    }
    // Remaining 1-3 bytes (little-endian tail).
    let rem = len - i;
    if rem > 0 {
        let mut k: u32 = 0;
        if rem >= 3 {
            k ^= (blocks[i + 2] as u32) << 16;
        }
        if rem >= 2 {
            k ^= (blocks[i + 1] as u32) << 8;
        }
        if rem >= 1 {
            k ^= blocks[i] as u32;
        }
        hash ^= k;
        hash = hash.wrapping_mul(M);
    }

    hash ^= hash >> 13;
    hash = hash.wrapping_mul(M);
    hash ^= hash >> 15;
    hash
}

/// Convenience wrapper that hashes a file on disk.
pub fn murmur2_file(path: &std::path::Path) -> std::io::Result<u32> {
    let file = std::fs::File::open(path)?;
    Ok(murmur2(file))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vector_matches_reference() {
        // Canonical MurmurHash2 (x86_32, seed=1) of "Hello, world!".
        let data = b"Hello, world!";
        let hash = murmur2(&data[..]);
        assert_eq!(hash, 0x63a27f49);
    }

    #[test]
    fn empty_input_is_seed() {
        let hash = murmur2(&[][..]);
        assert_eq!(hash, 0x5bd15e36);
    }
}
