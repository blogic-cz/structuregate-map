//! NTFS USN change journal reader (Windows only).
//!
//! NTFS records every change to every file on a volume as a numbered entry in a
//! ring buffer. A snapshot stores the USN it was taken at; the next run reads
//! only the entries after that number. Cost is proportional to the number of
//! changes, not to the size of the tree, so a 500k file tree with three edited
//! files is answered in milliseconds.
//!
//! Two conditions make the stored position useless, and both are checked before
//! any record is trusted. Either one means the caller must fall back to a full
//! walk.
//!
//! * The journal was deleted and recreated, so `UsnJournalID` differs.
//! * The ring buffer wrapped past the stored USN, so `FirstUsn` is higher.
//!
//! Reading the journal needs administrator rights on most systems.

use anyhow::{bail, Context, Result};
use std::ffi::c_void;
use std::path::Path;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, MAX_PATH};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, GetFinalPathNameByHandleW, OpenFileById, FILE_FLAGS_AND_ATTRIBUTES,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_ID_DESCRIPTOR, FILE_ID_DESCRIPTOR_0, FILE_ID_TYPE,
    FILE_NAME_NORMALIZED, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Ioctl::{
    FSCTL_QUERY_USN_JOURNAL, FSCTL_READ_USN_JOURNAL, READ_USN_JOURNAL_DATA_V0,
};
use windows::Win32::System::IO::DeviceIoControl;

/// Every reason is requested. Filtering happens later, against the stored map,
/// so a reason the filter did not anticipate cannot silently lose a change.
pub const REASON_MASK: u32 = 0xFFFF_FFFF;

pub const USN_REASON_FILE_CREATE: u32 = 0x0000_0100;
pub const USN_REASON_RENAME_NEW_NAME: u32 = 0x0000_2000;

const FILE_ATTRIBUTE_DIRECTORY_BIT: u32 = 0x10;

/// A handle that closes itself.
pub struct Volume {
    handle: HANDLE,
    pub name: String,
}

impl Drop for Volume {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

/// Position and identity of a volume's journal.
#[derive(Clone, Copy, Debug)]
pub struct JournalInfo {
    pub journal_id: u64,
    /// Oldest USN still in the ring buffer.
    pub first_usn: i64,
    /// USN the next change will get. This is what a snapshot stores.
    pub next_usn: i64,
}

/// One collapsed change: all records for the same node folded into one row.
#[derive(Clone, Debug)]
pub struct UsnChange {
    pub file_ref: u64,
    pub parent_ref: u64,
    pub name: String,
    pub reason: u32,
    pub attributes: u32,
}

impl UsnChange {
    pub fn is_dir(&self) -> bool {
        self.attributes & FILE_ATTRIBUTE_DIRECTORY_BIT != 0
    }
    pub fn created_or_moved_in(&self) -> bool {
        self.reason & (USN_REASON_FILE_CREATE | USN_REASON_RENAME_NEW_NAME) != 0
    }
}

/// The `C:` style volume name a path sits on.
pub fn volume_of(path: &Path) -> Result<String> {
    let abs = std::fs::canonicalize(path)
        .with_context(|| format!("cannot resolve {}", path.display()))?;
    let s = abs.to_string_lossy().replace("\\\\?\\", "");
    let bytes = s.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' {
        Ok(format!("{}:", (bytes[0] as char).to_ascii_uppercase()))
    } else {
        bail!("{} is not on a drive letter volume", path.display())
    }
}

/// Open the raw volume device. Needs administrator rights.
pub fn open_volume(volume: &str) -> Result<Volume> {
    let device = format!("\\\\.\\{volume}");
    let wide: Vec<u16> = device.encode_utf16().chain(std::iter::once(0)).collect();
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            0x0001, // FILE_READ_DATA
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    }
    .with_context(|| format!("cannot open volume {volume}; administrator rights are required"))?;
    Ok(Volume { handle, name: volume.to_string() })
}

pub fn query_journal(vol: &Volume) -> Result<JournalInfo> {
    // Ask with a V2-sized buffer and read only the V0 prefix, which every
    // version starts with.
    let mut buf = [0u8; 96];
    let mut returned = 0u32;
    unsafe {
        DeviceIoControl(
            vol.handle,
            FSCTL_QUERY_USN_JOURNAL,
            None,
            0,
            Some(buf.as_mut_ptr() as *mut c_void),
            buf.len() as u32,
            Some(&mut returned),
            None,
        )
    }
    .context("FSCTL_QUERY_USN_JOURNAL failed; the volume may have no journal")?;

    if returned < 32 {
        bail!("journal query returned {returned} bytes, too short");
    }
    Ok(JournalInfo {
        journal_id: u64::from_le_bytes(buf[0..8].try_into()?),
        first_usn: i64::from_le_bytes(buf[8..16].try_into()?),
        next_usn: i64::from_le_bytes(buf[16..24].try_into()?),
    })
}

/// Read every record after `start_usn`, collapsed to one row per node.
///
/// Returns the changes and the USN to store for the next run.
pub fn read_changes(
    vol: &Volume,
    info: &JournalInfo,
    start_usn: i64,
) -> Result<(Vec<UsnChange>, i64)> {
    if start_usn < info.first_usn {
        bail!("journal wrapped past USN {start_usn}; a full scan is needed");
    }

    let mut merged: std::collections::HashMap<(u64, String), UsnChange> =
        std::collections::HashMap::new();
    let mut next = start_usn;
    let mut buf = vec![0u64; 128 * 1024 / 8];

    loop {
        let query = READ_USN_JOURNAL_DATA_V0 {
            StartUsn: next,
            ReasonMask: REASON_MASK,
            ReturnOnlyOnClose: 0,
            Timeout: 0,
            BytesToWaitFor: 0,
            UsnJournalID: info.journal_id,
        };
        let mut returned = 0u32;
        unsafe {
            DeviceIoControl(
                vol.handle,
                FSCTL_READ_USN_JOURNAL,
                Some(&query as *const _ as *const c_void),
                std::mem::size_of::<READ_USN_JOURNAL_DATA_V0>() as u32,
                Some(buf.as_mut_ptr() as *mut c_void),
                (buf.len() * 8) as u32,
                Some(&mut returned),
                None,
            )
        }
        .context("FSCTL_READ_USN_JOURNAL failed")?;

        // The first 8 bytes are the USN to continue from. Nothing beyond them
        // means the journal is drained.
        if returned <= 8 {
            break;
        }
        let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, returned as usize) };
        let continue_usn = i64::from_le_bytes(bytes[0..8].try_into()?);

        let mut offset = 8usize;
        while offset + 60 <= bytes.len() {
            let len = u32::from_le_bytes(bytes[offset..offset + 4].try_into()?) as usize;
            if len == 0 || offset + len > bytes.len() {
                break;
            }
            if let Some(c) = parse_record(&bytes[offset..offset + len]) {
                merged
                    .entry((c.file_ref, c.name.to_lowercase()))
                    .and_modify(|e| {
                        e.reason |= c.reason;
                        e.attributes = c.attributes;
                        e.parent_ref = c.parent_ref;
                    })
                    .or_insert(c);
            }
            offset += len;
        }

        if continue_usn <= next {
            break;
        }
        next = continue_usn;
    }

    Ok((merged.into_values().collect(), next))
}

/// Parse one USN record.
///
/// Version 2 carries 64-bit file references, version 3 carries 128-bit ones. On
/// NTFS the low 64 bits of a 128-bit reference are the same number the directory
/// enumeration reports as the file id, so both versions map onto the same key.
fn parse_record(rec: &[u8]) -> Option<UsnChange> {
    let major = u16::from_le_bytes(rec[4..6].try_into().ok()?);
    let (file_ref, parent_ref, reason_at, attrs_at, name_len_at) = match major {
        2 => (
            u64::from_le_bytes(rec.get(8..16)?.try_into().ok()?),
            u64::from_le_bytes(rec.get(16..24)?.try_into().ok()?),
            40,
            52,
            56,
        ),
        3 | 4 => (
            u64::from_le_bytes(rec.get(8..16)?.try_into().ok()?),
            u64::from_le_bytes(rec.get(24..32)?.try_into().ok()?),
            56,
            68,
            72,
        ),
        _ => return None,
    };

    let reason = u32::from_le_bytes(rec.get(reason_at..reason_at + 4)?.try_into().ok()?);
    let attributes = u32::from_le_bytes(rec.get(attrs_at..attrs_at + 4)?.try_into().ok()?);
    let name_len = u16::from_le_bytes(rec.get(name_len_at..name_len_at + 2)?.try_into().ok()?) as usize;
    let name_off =
        u16::from_le_bytes(rec.get(name_len_at + 2..name_len_at + 4)?.try_into().ok()?) as usize;

    let units: Vec<u16> = rec
        .get(name_off..name_off + name_len)?
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    Some(UsnChange {
        file_ref,
        parent_ref,
        name: String::from_utf16_lossy(&units),
        reason,
        attributes,
    })
}

/// Human readable reason flags, for the `journal` command.
pub fn reason_names(reason: u32) -> Vec<&'static str> {
    const FLAGS: &[(u32, &str)] = &[
        (0x0000_0001, "DATA_OVERWRITE"),
        (0x0000_0002, "DATA_EXTEND"),
        (0x0000_0004, "DATA_TRUNCATION"),
        (0x0000_0010, "NAMED_DATA_OVERWRITE"),
        (0x0000_0020, "NAMED_DATA_EXTEND"),
        (0x0000_0040, "NAMED_DATA_TRUNCATION"),
        (0x0000_0100, "FILE_CREATE"),
        (0x0000_0200, "FILE_DELETE"),
        (0x0000_0400, "EA_CHANGE"),
        (0x0000_0800, "SECURITY_CHANGE"),
        (0x0000_1000, "RENAME_OLD_NAME"),
        (0x0000_2000, "RENAME_NEW_NAME"),
        (0x0000_4000, "INDEXABLE_CHANGE"),
        (0x0000_8000, "BASIC_INFO_CHANGE"),
        (0x0001_0000, "HARD_LINK_CHANGE"),
        (0x0002_0000, "COMPRESSION_CHANGE"),
        (0x0004_0000, "ENCRYPTION_CHANGE"),
        (0x0008_0000, "OBJECT_ID_CHANGE"),
        (0x0010_0000, "REPARSE_POINT_CHANGE"),
        (0x0020_0000, "STREAM_CHANGE"),
        (0x0040_0000, "TRANSACTED_CHANGE"),
        (0x0080_0000, "INTEGRITY_CHANGE"),
        (0x8000_0000, "CLOSE"),
    ];
    FLAGS
        .iter()
        .filter(|(bit, _)| reason & bit != 0)
        .map(|(_, name)| *name)
        .collect()
}

/// Resolve a file reference number to a full path.
///
/// Only used for nodes the stored map does not know yet, such as a directory
/// created since the last scan. It costs two syscalls, so the caller should
/// resolve through the stored parent map wherever it can.
pub fn path_of_ref(vol: &Volume, file_ref: u64) -> Option<String> {
    let desc = FILE_ID_DESCRIPTOR {
        dwSize: std::mem::size_of::<FILE_ID_DESCRIPTOR>() as u32,
        Type: FILE_ID_TYPE(0), // FileIdType
        Anonymous: FILE_ID_DESCRIPTOR_0 { FileId: file_ref as i64 },
    };
    let handle = unsafe {
        OpenFileById(
            vol.handle,
            &desc,
            0x0080, // FILE_READ_ATTRIBUTES
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            FILE_FLAG_BACKUP_SEMANTICS,
        )
    }
    .ok()?;

    let mut buf = vec![0u16; MAX_PATH as usize * 4];
    let len = unsafe { GetFinalPathNameByHandleW(handle, &mut buf, FILE_NAME_NORMALIZED) };
    unsafe {
        let _ = CloseHandle(handle);
    }
    if len == 0 || len as usize > buf.len() {
        return None;
    }
    let s = String::from_utf16_lossy(&buf[..len as usize]);
    Some(s.trim_start_matches("\\\\?\\").to_string())
}
