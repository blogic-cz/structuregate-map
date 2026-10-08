//! Core node type stored in the tree map, plus path normalisation.

/// What a node is. Stored as an integer in SQLite.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Kind {
    Dir = 0,
    File = 1,
    Link = 2,
}

impl Kind {
    pub fn from_i64(v: i64) -> Kind {
        match v {
            0 => Kind::Dir,
            2 => Kind::Link,
            _ => Kind::File,
        }
    }
}

/// A 32-byte BLAKE3 digest. Files hold the content hash, directories the merkle
/// hash of their children.
pub type Hash = [u8; 32];

/// One node of the tree map.
#[derive(Clone, Debug)]
pub struct Entry {
    /// Path relative to the scan root, `/` separated, original case.
    pub path: String,
    /// `path` lowercased. All comparisons and joins use this, because Windows
    /// paths are case insensitive.
    pub path_key: String,
    /// `path_key` of the containing directory. `None` for the root itself.
    pub parent_key: Option<String>,
    pub kind: Kind,
    pub size: u64,
    /// Modification time in nanoseconds since the Unix epoch.
    pub mtime_ns: i64,
    /// NTFS 64-bit file reference number. `None` on platforms or walkers that
    /// cannot supply it for free. Used for rename detection and USN mapping.
    pub file_id: Option<u64>,
    pub hash: Option<Hash>,
}

impl Entry {
    pub fn depth(&self) -> usize {
        if self.path_key.is_empty() {
            0
        } else {
            self.path_key.bytes().filter(|b| *b == b'/').count() + 1
        }
    }

    pub fn name(&self) -> &str {
        match self.path.rfind('/') {
            Some(i) => &self.path[i + 1..],
            None => &self.path,
        }
    }
}

/// Turn a native relative path into the canonical `/`-separated form.
pub fn normalize(path: &str) -> String {
    path.replace('\\', "/").trim_matches('/').to_string()
}

/// Comparison key for a normalised path.
pub fn key_of(path: &str) -> String {
    path.to_lowercase()
}

/// Parent of a node, as a key.
///
/// The scan root is the empty key and is the only node without a parent. A node
/// at the top level therefore gets `Some("")`, not `None`. Returning `None`
/// there would cut every top-level node out of the root's child list, and the
/// root hash would then never change.
pub fn parent_key_of(path_key: &str) -> Option<String> {
    if path_key.is_empty() {
        return None;
    }
    Some(match path_key.rfind('/') {
        Some(i) => path_key[..i].to_string(),
        None => String::new(),
    })
}

/// Windows FILETIME (100 ns ticks since 1601-01-01) to Unix nanoseconds.
pub fn filetime_to_unix_ns(ft: i64) -> i64 {
    const EPOCH_DIFF_TICKS: i64 = 116_444_736_000_000_000;
    ft.saturating_sub(EPOCH_DIFF_TICKS).saturating_mul(100)
}

/// Current wall clock in Unix nanoseconds.
pub fn now_unix_ns() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

/// `SystemTime` to Unix nanoseconds, for the portable walker.
pub fn systemtime_to_unix_ns(t: std::time::SystemTime) -> i64 {
    use std::time::UNIX_EPOCH;
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_nanos() as i64,
        Err(e) => -(e.duration().as_nanos() as i64),
    }
}
