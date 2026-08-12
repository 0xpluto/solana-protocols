//! On-disk format for `LocalCache` snapshots.
//!
//! File layout:
//!
//! ```text
//! [magic:  4 bytes "STCH"]
//! [version: u32 little-endian]
//! [payload: gzip(bincode(T))]
//! ```
//!
//! The magic + version header is written uncompressed so a version mismatch
//! or unrelated file is detected before any decompression or deserialization
//! work. Writes are atomic (temp file + rename on the same directory).
//!
//! `T` is whatever the caller wants to persist — currently `CacheData`. The
//! read/write helpers are generic so we don't maintain a second mirror of
//! the field table purely for serialization.

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::de::DeserializeOwned;
use serde::Serialize;

pub(crate) const MAGIC: [u8; 4] = *b"STCH";
pub(crate) const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("i/o error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("bincode error at {path}: {source}")]
    Serde {
        path: PathBuf,
        #[source]
        source: bincode::Error,
    },
    #[error("{path}: file does not start with STCH magic (got {magic:?})")]
    BadMagic { path: PathBuf, magic: [u8; 4] },
    #[error("{path}: unsupported schema version {found} (this build expects {expected})")]
    UnsupportedVersion {
        path: PathBuf,
        expected: u32,
        found: u32,
    },
    #[error("no persist path configured on this LocalCache")]
    NoPersistPath,
}

impl CacheError {
    fn io(path: &Path, source: std::io::Error) -> Self {
        Self::Io {
            path: path.to_path_buf(),
            source,
        }
    }

    fn serde(path: &Path, source: bincode::Error) -> Self {
        Self::Serde {
            path: path.to_path_buf(),
            source,
        }
    }
}

/// Serialize `payload` to `path` in the cache's on-disk format.
pub(crate) fn write_payload<T: Serialize>(path: &Path, payload: &T) -> Result<(), CacheError> {
    let tmp = tmp_path(path);
    let mut encoder = {
        let file = File::create(&tmp).map_err(|e| CacheError::io(&tmp, e))?;
        let mut writer = BufWriter::new(file);
        writer
            .write_all(&MAGIC)
            .map_err(|e| CacheError::io(&tmp, e))?;
        writer
            .write_all(&SCHEMA_VERSION.to_le_bytes())
            .map_err(|e| CacheError::io(&tmp, e))?;
        GzEncoder::new(writer, Compression::fast())
    };
    bincode::serialize_into(&mut encoder, payload).map_err(|e| CacheError::serde(&tmp, e))?;
    let mut writer = encoder.finish().map_err(|e| CacheError::io(&tmp, e))?;
    writer.flush().map_err(|e| CacheError::io(&tmp, e))?;
    drop(writer);

    fs::rename(&tmp, path).map_err(|e| CacheError::io(path, e))?;
    Ok(())
}

/// Read and deserialize a payload from `path`, validating the header first.
pub(crate) fn read_payload<T: DeserializeOwned>(path: &Path) -> Result<T, CacheError> {
    let file = File::open(path).map_err(|e| CacheError::io(path, e))?;
    let mut reader = BufReader::new(file);

    let mut magic = [0u8; 4];
    reader
        .read_exact(&mut magic)
        .map_err(|e| CacheError::io(path, e))?;
    if magic != MAGIC {
        return Err(CacheError::BadMagic {
            path: path.to_path_buf(),
            magic,
        });
    }

    let mut version_bytes = [0u8; 4];
    reader
        .read_exact(&mut version_bytes)
        .map_err(|e| CacheError::io(path, e))?;
    let version = u32::from_le_bytes(version_bytes);
    if version != SCHEMA_VERSION {
        return Err(CacheError::UnsupportedVersion {
            path: path.to_path_buf(),
            expected: SCHEMA_VERSION,
            found: version,
        });
    }

    let decoder = GzDecoder::new(reader);
    bincode::deserialize_from(decoder).map_err(|e| CacheError::serde(path, e))
}

fn tmp_path(path: &Path) -> PathBuf {
    let mut os = path.as_os_str().to_owned();
    os.push(".tmp");
    PathBuf::from(os)
}
