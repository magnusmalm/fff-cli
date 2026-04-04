//! On-disk binary format for the fff index.
//!
//! Layout is designed to be mmap-friendly: lookup tables and bitsets
//! are stored as flat arrays of native-endian integers.

use crate::error::{CliError, Result};
use fff::BigramFilter;
use fff::types::FileItem;
use std::io::Write;
use std::path::Path;

// ── Manifest ───────────────────────────────────────────────────────────

const MANIFEST_MAGIC: &[u8; 4] = b"FFFM";
const MANIFEST_VERSION: u32 = 1;

#[derive(Debug)]
pub struct IndexManifest {
    pub file_count: u32,
    pub base_path_hash: [u8; 32],
    pub git_head: [u8; 20],
    pub created_at: u64,
}

pub fn write_manifest(path: &Path, m: &IndexManifest) -> Result<()> {
    let mut f = std::fs::File::create(path)?;
    f.write_all(MANIFEST_MAGIC)?;
    f.write_all(&MANIFEST_VERSION.to_le_bytes())?;
    f.write_all(&m.file_count.to_le_bytes())?;
    f.write_all(&m.base_path_hash)?;
    f.write_all(&m.git_head)?;
    f.write_all(&m.created_at.to_le_bytes())?;
    f.flush()?;
    Ok(())
}

pub fn read_manifest(path: &Path) -> Result<IndexManifest> {
    let data = std::fs::read(path)?;
    let p = path.to_path_buf();
    let corrupt = |reason: &str| CliError::CorruptIndex {
        path: p.clone(),
        reason: reason.to_string(),
    };

    if data.len() < 4 + 4 + 4 + 32 + 20 + 8 {
        return Err(corrupt("manifest too short"));
    }
    if &data[0..4] != MANIFEST_MAGIC {
        return Err(corrupt("bad magic"));
    }
    let version = u32::from_le_bytes(data[4..8].try_into().unwrap());
    if version != MANIFEST_VERSION {
        return Err(corrupt(&format!("unsupported version {version}")));
    }

    let file_count = u32::from_le_bytes(data[8..12].try_into().unwrap());
    let mut base_path_hash = [0u8; 32];
    base_path_hash.copy_from_slice(&data[12..44]);
    let mut git_head = [0u8; 20];
    git_head.copy_from_slice(&data[44..64]);
    let created_at = u64::from_le_bytes(data[64..72].try_into().unwrap());

    Ok(IndexManifest {
        file_count,
        base_path_hash,
        git_head,
        created_at,
    })
}

// ── File list v2 (zero-copy friendly) ──────────────────────────────────
//
// Layout:
//   Header (16 bytes):  magic "FFF2" | version u32 | file_count u32 | string_table_size u32
//   Records (N * 24):   path_offset u32 | path_len u16 | name_len u16 | size u64 | modified u64
//   String table:       all relative_path strings concatenated (no separators)
//
// file_name is the last `name_len` bytes of relative_path — no separate storage.
// On load: mmap the file, read fixed records, slice strings from the table.

const FILES_V2_MAGIC: &[u8; 4] = b"FFF2";
const FILES_V2_VERSION: u32 = 2;
const FILES_V2_HEADER: usize = 16;
const FILES_V2_RECORD: usize = 24;

/// Bit 0 of the flags byte packed into name_len's high bit would be fragile.
/// Instead we pack is_binary into the top bit of name_len (max path component
/// is 255 on most OS, so 15 bits is plenty).
const BINARY_FLAG: u16 = 0x8000;

pub fn write_file_list(path: &Path, files: &[FileItem]) -> Result<()> {
    let string_table_size: usize = files.iter().map(|f| f.relative_path.len()).sum();
    let records_size = files.len() * FILES_V2_RECORD;
    let total = FILES_V2_HEADER + records_size + string_table_size;

    let mut buf = Vec::with_capacity(total);

    // Header
    buf.extend_from_slice(FILES_V2_MAGIC);
    buf.extend_from_slice(&FILES_V2_VERSION.to_le_bytes());
    buf.extend_from_slice(&(files.len() as u32).to_le_bytes());
    buf.extend_from_slice(&(string_table_size as u32).to_le_bytes());

    // Records
    let mut str_offset = 0u32;
    for file in files {
        let path_len = file.relative_path.len() as u16;
        let mut name_len = file.file_name.len() as u16;
        if file.is_binary {
            name_len |= BINARY_FLAG;
        }
        buf.extend_from_slice(&str_offset.to_le_bytes());
        buf.extend_from_slice(&path_len.to_le_bytes());
        buf.extend_from_slice(&name_len.to_le_bytes());
        buf.extend_from_slice(&file.size.to_le_bytes());
        buf.extend_from_slice(&file.modified.to_le_bytes());
        str_offset += file.relative_path.len() as u32;
    }

    // String table
    for file in files {
        buf.extend_from_slice(file.relative_path.as_bytes());
    }

    debug_assert_eq!(buf.len(), total);
    std::fs::write(path, &buf)?;
    Ok(())
}

pub fn read_file_list(path: &Path, base_path: &Path) -> Result<Vec<FileItem>> {
    // mmap the file — the kernel page cache does the heavy lifting.
    let file = std::fs::File::open(path)?;
    let data = unsafe { memmap2::Mmap::map(&file)? };

    let p = path.to_path_buf();
    let corrupt = |reason: &str| CliError::CorruptIndex {
        path: p.clone(),
        reason: reason.to_string(),
    };

    if data.len() < FILES_V2_HEADER {
        return Err(corrupt("file list too short"));
    }
    if &data[0..4] != FILES_V2_MAGIC {
        return Err(corrupt("bad files magic"));
    }
    let version = u32::from_le_bytes(data[4..8].try_into().unwrap());
    if version != FILES_V2_VERSION {
        return Err(corrupt(&format!("unsupported files version {version}")));
    }

    let count = u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
    let string_table_size = u32::from_le_bytes(data[12..16].try_into().unwrap()) as usize;

    let records_start = FILES_V2_HEADER;
    let records_end = records_start + count * FILES_V2_RECORD;
    let strings_start = records_end;
    let strings_end = strings_start + string_table_size;

    if data.len() < strings_end {
        return Err(corrupt("file list truncated"));
    }

    let strings = &data[strings_start..strings_end];
    let base_bytes = base_path.as_os_str().as_encoded_bytes();

    let mut files = Vec::with_capacity(count);

    for i in 0..count {
        let rec = records_start + i * FILES_V2_RECORD;
        let path_offset = u32::from_le_bytes(data[rec..rec + 4].try_into().unwrap()) as usize;
        let path_len = u16::from_le_bytes(data[rec + 4..rec + 6].try_into().unwrap()) as usize;
        let raw_name_len = u16::from_le_bytes(data[rec + 6..rec + 8].try_into().unwrap());
        let is_binary = raw_name_len & BINARY_FLAG != 0;
        let name_len = (raw_name_len & !BINARY_FLAG) as usize;
        let size = u64::from_le_bytes(data[rec + 8..rec + 16].try_into().unwrap());
        let modified = u64::from_le_bytes(data[rec + 16..rec + 24].try_into().unwrap());

        let path_bytes = &strings[path_offset..path_offset + path_len];

        // SAFETY: we wrote valid UTF-8 during indexing.
        let relative_path = unsafe { String::from_utf8_unchecked(path_bytes.to_vec()) };
        let file_name = if name_len > 0 && name_len <= path_len {
            unsafe { String::from_utf8_unchecked(path_bytes[path_len - name_len..].to_vec()) }
        } else {
            relative_path.clone()
        };

        // Build full path: base + "/" + relative (single allocation).
        let mut full = Vec::with_capacity(base_bytes.len() + 1 + path_len);
        full.extend_from_slice(base_bytes);
        full.push(b'/');
        full.extend_from_slice(path_bytes);
        let full_path = std::path::PathBuf::from(unsafe {
            std::ffi::OsString::from_encoded_bytes_unchecked(full)
        });

        files.push(FileItem::new_raw(
            full_path,
            relative_path,
            file_name,
            size,
            modified,
            None,
            is_binary,
        ));
    }

    Ok(files)
}

// ── Bigram index ───────────────────────────────────────────────────────

// Bigram index v2: lookup entries are u16 (matching upstream's -256KB optimization).
const BIGRAM_MAGIC: &[u8; 4] = b"FFB2";
const BIGRAM_VERSION: u32 = 2;

pub fn write_bigram_index(path: &Path, filter: &BigramFilter) -> Result<()> {
    let lookup = filter.lookup();
    let dense = filter.dense_data();
    let words = filter.words();
    let dense_count = filter.dense_count();
    let file_count = filter.file_count();
    let populated = filter.populated();

    // Header: magic(4) + version(4) + 5 * u32(20) = 28 bytes
    // Lookup: 65536 * 2 = 131072 bytes
    // Dense: dense_count * words * 8 bytes
    let total = 28 + lookup.len() * 2 + dense.len() * 8;
    let mut buf = Vec::with_capacity(total);

    buf.extend_from_slice(BIGRAM_MAGIC);
    buf.extend_from_slice(&BIGRAM_VERSION.to_le_bytes());
    buf.extend_from_slice(&(file_count as u32).to_le_bytes());
    buf.extend_from_slice(&(words as u32).to_le_bytes());
    buf.extend_from_slice(&(dense_count as u32).to_le_bytes());
    buf.extend_from_slice(&(populated as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes()); // reserved

    // Lookup table (u16 per entry)
    for &v in lookup {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    // Dense data
    for &v in dense {
        buf.extend_from_slice(&v.to_le_bytes());
    }

    std::fs::write(path, &buf)?;
    Ok(())
}

pub fn read_bigram_index(path: &Path) -> Result<BigramFilter> {
    let data = std::fs::read(path)?;
    let p = path.to_path_buf();
    let corrupt = |reason: &str| CliError::CorruptIndex {
        path: p.clone(),
        reason: reason.to_string(),
    };

    let header_size = 28;
    if data.len() < header_size {
        return Err(corrupt("bigram index too short"));
    }
    if &data[0..4] != BIGRAM_MAGIC {
        return Err(corrupt("bad bigram magic (rebuild index with `fff index --force`)"));
    }
    let version = u32::from_le_bytes(data[4..8].try_into().unwrap());
    if version != BIGRAM_VERSION {
        return Err(corrupt(&format!("unsupported bigram version {version}")));
    }

    let file_count = u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
    let words = u32::from_le_bytes(data[12..16].try_into().unwrap()) as usize;
    let dense_count = u32::from_le_bytes(data[16..20].try_into().unwrap()) as usize;
    let populated = u32::from_le_bytes(data[20..24].try_into().unwrap()) as usize;

    let lookup_start = header_size;
    let lookup_bytes = 65536 * 2; // u16 per entry
    let dense_start = lookup_start + lookup_bytes;
    let dense_bytes = dense_count * words * 8;

    if data.len() < dense_start + dense_bytes {
        return Err(corrupt("bigram data truncated"));
    }

    let mut lookup = vec![0u16; 65536];
    for i in 0..65536 {
        let off = lookup_start + i * 2;
        lookup[i] = u16::from_le_bytes(data[off..off + 2].try_into().unwrap());
    }

    let mut dense_data = vec![0u64; dense_count * words];
    for i in 0..dense_data.len() {
        let off = dense_start + i * 8;
        dense_data[i] = u64::from_le_bytes(data[off..off + 8].try_into().unwrap());
    }

    Ok(BigramFilter::new(
        lookup,
        dense_data,
        dense_count,
        words,
        file_count,
        populated,
    ))
}
