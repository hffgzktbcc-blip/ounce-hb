//! Clean-room NSO0 header parser and LZ4 block decompressor.
//!
//! Layout comes from the public switchbrew wiki NSO page (`https://switchbrew.org/wiki/NSO`), and
//! block decompression follows the public LZ4 block format specification
//! (`https://github.com/lz4/lz4/blob/dev/doc/lz4_Block_format.md`).
//!
//! devkitPro's `elf2nso` produces NSO0 executables with LZ4-compressed `.text`, `.rodata`, and
//! `.data` segments (header `flags` bits 0–2). Staging an NSO through [`decompress_nso`] produces
//! an equivalent uncompressed NSO0 image so backends that map uncompressed NSO0 segments directly
//! (such as `ounce-core run`) can execute `elf2nso`-built homebrew without external dependencies.

/// Size of the fixed `NSO0` header in bytes (switchbrew wiki NSO page).
pub const NSO_HEADER_SIZE: usize = 0x100;

/// Upper bound on any single decompressed NSO segment (64 MiB) to prevent unbounded allocation
/// from malformed headers.
pub const MAX_NSO_SEGMENT_BYTES: usize = 64 * 1024 * 1024;

const FLAG_TEXT_COMPRESSED: u32 = 1 << 0;
const FLAG_RO_COMPRESSED: u32 = 1 << 1;
const FLAG_DATA_COMPRESSED: u32 = 1 << 2;
const COMPRESSION_MASK: u32 = FLAG_TEXT_COMPRESSED | FLAG_RO_COMPRESSED | FLAG_DATA_COMPRESSED;

fn read_u32_le(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

/// Validates an `NSO0` image and decompresses any LZ4-compressed segments (header `flags` bits
/// 0–2) into a standard uncompressed `NSO0` image.
///
/// Uncompressed NSO0 images are validated for segment bounds and returned unchanged.
pub fn decompress_nso(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() < NSO_HEADER_SIZE {
        return Err(format!(
            "NSO0 header is truncated ({} bytes, need at least {NSO_HEADER_SIZE}).",
            bytes.len()
        ));
    }
    if &bytes[0..4] != b"NSO0" {
        return Err("not an NSO0 image (missing NSO0 magic).".to_string());
    }

    let flags = read_u32_le(bytes, 0x0C);
    let specs: [(&str, usize, usize, u32); 3] = [
        ("text", 0x10, 0x60, FLAG_TEXT_COMPRESSED),
        ("rodata", 0x20, 0x64, FLAG_RO_COMPRESSED),
        ("data", 0x30, 0x68, FLAG_DATA_COMPRESSED),
    ];

    let mut segments: Vec<(usize, usize, u32, Vec<u8>)> = Vec::with_capacity(3);
    for (name, seg_off, fsize_off, flag_bit) in specs {
        let file_off = read_u32_le(bytes, seg_off) as usize;
        let mem_off = read_u32_le(bytes, seg_off + 4);
        let decompressed_size = read_u32_le(bytes, seg_off + 8) as usize;
        let compressed_size = read_u32_le(bytes, fsize_off) as usize;

        if decompressed_size > MAX_NSO_SEGMENT_BYTES {
            return Err(format!(
                "NSO0 {name} segment decompressed size ({decompressed_size} bytes) exceeds limit ({MAX_NSO_SEGMENT_BYTES} bytes)."
            ));
        }

        let is_compressed = (flags & flag_bit) != 0;
        let stored_size = if is_compressed {
            compressed_size
        } else {
            decompressed_size
        };

        let raw_seg = if stored_size == 0 {
            &[][..]
        } else {
            let end = file_off
                .checked_add(stored_size)
                .filter(|&e| e <= bytes.len())
                .ok_or_else(|| {
                    format!(
                        "NSO0 {name} segment exceeds file bounds (offset 0x{file_off:x}, size 0x{stored_size:x}, file {} bytes).",
                        bytes.len()
                    )
                })?;
            &bytes[file_off..end]
        };

        let data = if is_compressed {
            lz4_decompress_block(raw_seg, decompressed_size)
                .map_err(|e| format!("NSO0 {name} LZ4 decompression failed: {e}"))?
        } else {
            raw_seg.to_vec()
        };

        segments.push((seg_off, fsize_off, mem_off, data));
    }

    if (flags & COMPRESSION_MASK) == 0 {
        return Ok(bytes.to_vec());
    }

    let mut out = bytes[..NSO_HEADER_SIZE].to_vec();
    out[0x0C..0x10].copy_from_slice(&(flags & !COMPRESSION_MASK).to_le_bytes());

    let mod_off = read_u32_le(bytes, 0x1C) as usize;
    let mod_size = read_u32_le(bytes, 0x2C) as usize;
    if mod_size > 0
        && mod_off >= NSO_HEADER_SIZE
        && let Some(end) = mod_off.checked_add(mod_size)
        && end <= bytes.len()
    {
        let new_mod_off = u32::try_from(out.len())
            .map_err(|_| "NSO0 output exceeds 32-bit offset range.".to_string())?;
        out[0x1C..0x20].copy_from_slice(&new_mod_off.to_le_bytes());
        out.extend_from_slice(&bytes[mod_off..end]);
    }

    for (seg_off, fsize_off, mem_off, data) in segments {
        let new_off = u32::try_from(out.len())
            .map_err(|_| "NSO0 output exceeds 32-bit offset range.".to_string())?;
        let new_len = u32::try_from(data.len())
            .map_err(|_| "NSO0 segment exceeds 32-bit length range.".to_string())?;
        out[seg_off..seg_off + 4].copy_from_slice(&new_off.to_le_bytes());
        out[seg_off + 4..seg_off + 8].copy_from_slice(&mem_off.to_le_bytes());
        out[seg_off + 8..seg_off + 12].copy_from_slice(&new_len.to_le_bytes());
        out[fsize_off..fsize_off + 4].copy_from_slice(&new_len.to_le_bytes());
        out.extend_from_slice(&data);
    }

    Ok(out)
}

/// Decompresses a single raw LZ4 block into an output buffer of exact length `expected_len`.
///
/// Follows the public LZ4 block format specification (`lz4_Block_format.md`): each sequence starts
/// with a token byte (`high nibble` = literal length, `low nibble` = match length minus 4),
/// optional 255-extended lengths, literal bytes, and (for all sequences except the last) a 16-bit
/// little-endian back-reference offset.
pub fn lz4_decompress_block(src: &[u8], expected_len: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(expected_len);
    let mut i = 0usize;

    while i < src.len() {
        let token = src[i];
        i += 1;

        let mut lit_len = (token >> 4) as usize;
        if lit_len == 15 {
            loop {
                let b = *src
                    .get(i)
                    .ok_or_else(|| "truncated literal length".to_string())?;
                i += 1;
                lit_len = lit_len
                    .checked_add(b as usize)
                    .ok_or_else(|| "literal length overflow".to_string())?;
                if b != 255 {
                    break;
                }
            }
        }

        let lit_end = i
            .checked_add(lit_len)
            .filter(|&e| e <= src.len())
            .ok_or_else(|| "literal extends past end of input".to_string())?;
        if out
            .len()
            .checked_add(lit_len)
            .is_none_or(|n| n > expected_len)
        {
            return Err("literal overflows expected output size".to_string());
        }
        out.extend_from_slice(&src[i..lit_end]);
        i = lit_end;

        if i == src.len() {
            break;
        }

        if i + 2 > src.len() {
            return Err("truncated match offset".to_string());
        }
        let offset = u16::from_le_bytes([src[i], src[i + 1]]) as usize;
        i += 2;
        if offset == 0 || offset > out.len() {
            return Err(format!(
                "invalid match offset {offset} at output position {}",
                out.len()
            ));
        }

        let match_base = (token & 0x0F) as usize;
        let mut match_len = match_base + 4;
        if match_base == 15 {
            loop {
                let b = *src
                    .get(i)
                    .ok_or_else(|| "truncated match length".to_string())?;
                i += 1;
                match_len = match_len
                    .checked_add(b as usize)
                    .ok_or_else(|| "match length overflow".to_string())?;
                if b != 255 {
                    break;
                }
            }
        }

        if out
            .len()
            .checked_add(match_len)
            .is_none_or(|n| n > expected_len)
        {
            return Err("match overflows expected output size".to_string());
        }
        for _ in 0..match_len {
            let byte = out[out.len() - offset];
            out.push(byte);
        }
    }

    if out.len() != expected_len {
        return Err(format!(
            "decompressed length {} does not match expected {expected_len}",
            out.len()
        ));
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lz4_decompresses_literals_and_overlapping_matches() {
        // Token: 2 literals ("AB"), match length = 0 + 4 = 4, offset = 2 -> "ABABAB".
        let block = [0x20u8, b'A', b'B', 0x02, 0x00];
        assert_eq!(lz4_decompress_block(&block, 6).unwrap(), b"ABABAB");
    }

    #[test]
    fn lz4_rejects_bad_offset_and_truncation() {
        assert!(lz4_decompress_block(&[0x10, b'A', 0x00, 0x00], 5).is_err());
        assert!(lz4_decompress_block(&[0x10, b'A', 0x05, 0x00], 5).is_err());
        assert!(lz4_decompress_block(&[0xF0], 20).is_err());
    }

    #[test]
    fn uncompressed_nso_passes_through_unchanged() {
        let mut nso = vec![0u8; NSO_HEADER_SIZE + 8];
        nso[0..4].copy_from_slice(b"NSO0");
        nso[0x10..0x14].copy_from_slice(&(NSO_HEADER_SIZE as u32).to_le_bytes());
        nso[0x18..0x1C].copy_from_slice(&8u32.to_le_bytes());
        nso[0x60..0x64].copy_from_slice(&8u32.to_le_bytes());
        nso[NSO_HEADER_SIZE..].copy_from_slice(b"12345678");
        assert_eq!(decompress_nso(&nso).unwrap(), nso);
    }

    #[test]
    fn compressed_nso_decompresses_and_clears_compression_flags() {
        let compressed_text = [0x20u8, b'A', b'B', 0x02, 0x00]; // -> "ABABAB" (6 bytes)
        let mut nso = vec![0u8; NSO_HEADER_SIZE + compressed_text.len()];
        nso[0..4].copy_from_slice(b"NSO0");
        nso[0x0C..0x10].copy_from_slice(&FLAG_TEXT_COMPRESSED.to_le_bytes());
        nso[0x10..0x14].copy_from_slice(&(NSO_HEADER_SIZE as u32).to_le_bytes());
        nso[0x18..0x1C].copy_from_slice(&6u32.to_le_bytes());
        nso[0x60..0x64].copy_from_slice(&(compressed_text.len() as u32).to_le_bytes());
        nso[NSO_HEADER_SIZE..].copy_from_slice(&compressed_text);

        let out = decompress_nso(&nso).unwrap();
        assert_eq!(&out[0..4], b"NSO0");
        assert_eq!(read_u32_le(&out, 0x0C) & COMPRESSION_MASK, 0);
        let text_off = read_u32_le(&out, 0x10) as usize;
        let text_len = read_u32_le(&out, 0x18) as usize;
        assert_eq!(text_len, 6);
        assert_eq!(read_u32_le(&out, 0x60) as usize, 6);
        assert_eq!(&out[text_off..text_off + text_len], b"ABABAB");
    }

    #[test]
    fn rejects_truncated_or_out_of_bounds_nso() {
        assert!(decompress_nso(b"NSO0").is_err());
        let mut bad = vec![0u8; NSO_HEADER_SIZE];
        bad[0..4].copy_from_slice(b"NSO0");
        bad[0x10..0x14].copy_from_slice(&(NSO_HEADER_SIZE as u32).to_le_bytes());
        bad[0x18..0x1C].copy_from_slice(&16u32.to_le_bytes());
        assert!(decompress_nso(&bad).is_err());
    }
}
