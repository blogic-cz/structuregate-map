//! HAS THIS TREE ALREADY PASSED? The gate runs before `CoreCompile` on every build of every consuming
//! project and walks the whole tree to answer a question that is a PURE FUNCTION of that tree AND OF THE
//! RULES. A tree byte-identical to one that passed still passes, so the pass is recorded under a key made
//! of the tree's own merkle root (from disk, by `tree.rs` - never out of the store, so nothing compares the
//! database against itself), the exe's version and the ARGUMENTS VERBATIM: every flag that changes a
//! verdict is in them by construction, where a list of "the flags that matter" goes stale the first time a
//! rule grows a knob.
//!
//! WHAT A MISS COSTS. If the journal ever fails to name a change, one build skips its gate and the next run
//! that sees a change catches it - bounded, and self-correcting.

use crate::tree;
use std::path::Path;



/// The pass key of one run over one tree, and whether it already passed.
pub(crate) struct Found {
    pub key: String,
    pub method: String,
    pub passed: bool,
}

/// `None` when the tree cannot be hashed - never a reason to fail the build, the gate simply walks.
pub(crate) fn find(root: &str, skip: &[String], tracked: bool, version: &str, arguments: &str) -> Option<Found> {
    let root = Path::new(root);
    let verdict = tree::look(root, skip, tracked).ok()?;
    if verdict.hash.is_empty() {
        return None;
    }
    let key = blake3::hash(format!("{} {version} {arguments}", verdict.hash).as_bytes()).to_hex().to_string();
    // The tree map answers about the TREE; whether that tree passed THESE rules is the second question.
    let passed = tree::passed_before(root, &key).unwrap_or(false);
    Some(Found { key, method: verdict.method, passed })
}

/// Record a pass - see `fbt_gate_pass`.
pub(crate) fn remember(root: &str, key: &str) {
    let _ = tree::remember(Path::new(root), key);
}

