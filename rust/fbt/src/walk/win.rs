//! Fast NTFS walker.
//!
//! `GetFileInformationByHandleEx(FileIdBothDirectoryInfo)` returns name, size,
//! timestamps, attributes *and* the 64-bit file id for a whole directory in one
//! call per 64 KiB buffer. That removes the per-file `stat` the portable walker
//! needs, and the file id it hands over for free is what makes rename detection
//! and the USN journal fast path work.

use super::{Filter, RawEntry};
use crate::entry::{filetime_to_unix_ns, Kind};
use anyhow::{bail, Result};
use rayon::prelude::*;
use std::ffi::c_void;
use std::mem::offset_of;
use std::path::Path;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, ERROR_NO_MORE_FILES, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, GetFileInformationByHandle, GetFileInformationByHandleEx, FileIdBothDirectoryInfo,
    FileIdBothDirectoryRestartInfo, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS, FILE_ID_BOTH_DIR_INFO,
    FILE_LIST_DIRECTORY, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};

/// One call per this many bytes. Bigger buffers mean fewer syscalls per
/// directory; 64 KiB holds a few hundred entries.
const BUF_BYTES: usize = 64 * 1024;

/// A handle that closes itself, so an early return cannot leak it.
struct Dir(HANDLE);

impl Drop for Dir {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Build a UTF-16, NUL terminated, `\\?\` prefixed path so long paths work.
fn wide_path(p: &Path) -> Vec<u16> {
    let s = p.to_string_lossy().replace('/', "\\");
    let s = if s.starts_with("\\\\?\\") || s.starts_with("\\\\.\\") {
        s
    } else if let Some(unc) = s.strip_prefix("\\\\") {
        format!("\\\\?\\UNC\\{unc}")
    } else {
        format!("\\\\?\\{s}")
    };
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn open_dir(path: &Path) -> Result<Dir> {
    let wide = wide_path(path);
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_LIST_DIRECTORY.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            None,
        )?
    };
    Ok(Dir(handle))
}

/// The 64-bit NTFS file reference of one path.
///
/// The directory enumeration hands this over for free for every child, so this
/// is only needed for the scan root itself and for nodes the USN fast path has
/// to look at one at a time.
pub fn file_id_of(path: &Path) -> Result<u64> {
    let dir = open_dir(path)?;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    unsafe { GetFileInformationByHandle(dir.0, &mut info)? };
    Ok(((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64)
}

pub fn walk(root: &Path, filter: &Filter) -> Result<Vec<RawEntry>> {
    if !root.is_dir() {
        bail!("{} is not a directory", root.display());
    }
    // Fail early, before spawning work, if this volume rejects the fast path.
    let _probe = open_dir(root)?;
    drop(_probe);

    let mut out = vec![RawEntry {
        rel: String::new(),
        kind: Kind::Dir,
        size: 0,
        mtime_ns: 0,
        // The root needs its own id, because the USN fast path resolves a record
        // by its parent reference and files sit directly in the root.
        file_id: file_id_of(root).ok(),
        link_target: None,
    }];

    let mut level = vec![String::new()];
    while !level.is_empty() {
        let results: Vec<(Vec<RawEntry>, Vec<String>)> = level
            .par_iter()
            .map(|rel| read_one(root, rel, filter).unwrap_or_default())
            .collect();

        let mut next = Vec::new();
        for (entries, dirs) in results {
            out.extend(entries);
            next.extend(dirs);
        }
        level = next;
    }
    Ok(out)
}

/// Enumerate one directory. Returns its entries and the subdirectories to visit.
fn read_one(root: &Path, rel: &str, filter: &Filter) -> Result<(Vec<RawEntry>, Vec<String>)> {
    let dir_path = if rel.is_empty() { root.to_path_buf() } else { root.join(rel) };
    let dir = open_dir(&dir_path)?;

    // u64 elements guarantee the 8-byte alignment the LARGE_INTEGER fields need.
    let mut buf = vec![0u64; BUF_BYTES / 8];
    let mut entries = Vec::new();
    let mut subdirs = Vec::new();
    let mut class = FileIdBothDirectoryRestartInfo;

    loop {
        let ok = unsafe {
            GetFileInformationByHandleEx(
                dir.0,
                class,
                buf.as_mut_ptr() as *mut c_void,
                (buf.len() * 8) as u32,
            )
        };
        if let Err(e) = ok {
            // The documented end of the enumeration, not a failure.
            if e.code() == ERROR_NO_MORE_FILES.to_hresult() {
                break;
            }
            return Err(e.into());
        }
        class = FileIdBothDirectoryInfo;

        let mut offset = 0usize;
        loop {
            let base = unsafe { (buf.as_ptr() as *const u8).add(offset) };
            let rec = unsafe { std::ptr::read_unaligned(base as *const FILE_ID_BOTH_DIR_INFO) };

            let name_len = rec.FileNameLength as usize / 2;
            let name_ptr =
                unsafe { base.add(offset_of!(FILE_ID_BOTH_DIR_INFO, FileName)) as *const u16 };
            let name_units = unsafe { std::slice::from_raw_parts(name_ptr, name_len) };
            let name = String::from_utf16_lossy(name_units);

            if name != "." && name != ".." {
                collect(root, rel, &name, &rec, filter, &mut entries, &mut subdirs);
            }

            if rec.NextEntryOffset == 0 {
                break;
            }
            offset += rec.NextEntryOffset as usize;
        }
    }
    Ok((entries, subdirs))
}

fn collect(
    root: &Path,
    rel: &str,
    name: &str,
    rec: &FILE_ID_BOTH_DIR_INFO,
    filter: &Filter,
    entries: &mut Vec<RawEntry>,
    subdirs: &mut Vec<String>,
) {
    let attrs = rec.FileAttributes;
    let is_reparse = attrs & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0;
    let is_dir = attrs & FILE_ATTRIBUTE_DIRECTORY.0 != 0 && !is_reparse;

    let child_rel = if rel.is_empty() { name.to_string() } else { format!("{rel}/{name}") };
    if filter.skip(&child_rel, name, is_dir) {
        return;
    }

    let kind = if is_reparse {
        Kind::Link
    } else if is_dir {
        Kind::Dir
    } else {
        Kind::File
    };

    // A junction or symlink is recorded by its target text. Following it would
    // risk a cycle and would hide the link being repointed.
    let link_target = if is_reparse {
        std::fs::read_link(root.join(&child_rel))
            .ok()
            .map(|p| p.to_string_lossy().to_string())
    } else {
        None
    };

    if is_dir {
        subdirs.push(child_rel.clone());
    }
    entries.push(RawEntry {
        rel: child_rel,
        kind,
        size: if kind == Kind::File { rec.EndOfFile as u64 } else { 0 },
        mtime_ns: filetime_to_unix_ns(rec.LastWriteTime),
        file_id: Some(rec.FileId as u64),
        link_target,
    });
}
