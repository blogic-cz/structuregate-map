//! fbt - fast source tree map and change detection over SQLite.
//!
//! A snapshot stores every node of a tree with its content hash, plus a merkle
//! hash per directory. A later run compares the tree against the stored map and
//! reports what moved, then maps the changed paths onto the build targets that
//! have to rebuild.

pub mod db;
pub mod diff;
pub mod entry;
pub mod hash;
pub mod pipeline;
pub mod scan;
pub mod targets;
pub mod walk;

#[cfg(windows)]
pub mod incremental;
#[cfg(windows)]
pub mod usn;
