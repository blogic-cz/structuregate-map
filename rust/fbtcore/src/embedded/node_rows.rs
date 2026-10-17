//! WHAT THE ANGULAR HALF'S STORED ROWS ARE A FUNCTION OF ON THIS SIDE: node's scripts and the rust code its payload
//! passes through on the way into the database, listed and hashed by `build.rs`.
//!
//! It is handed to node as `--build`, which node folds into its setup hash, and it is in the half's skip key. The
//! running build is NOT: a release that changes only what rust derives from the stored rows - the closure, the
//! stylesheet tables - leaves this value as it was, and the half derives those again from the rows already stored
//! (`rows/derive/tsderive.rs`) instead of starting node over the whole tree.

include!(concat!(env!("OUT_DIR"), "/node_rows.rs"));

/// STANDS IN, FOR A BLACK-BOX TEST, FOR AN EXTRACTOR CHANGE A TEST CANNOT BUILD: a script or a listed rust file only
/// changes with a new library, and a test runs the one it was handed. Folded in like either.
const TEST_SALT: &str = "STRUCTUREGATE_TEST_EXTRACTOR";

/// What produced the rows node writes and rust stores: the listed rust and the embedded scripts.
pub fn extractor() -> String {
    let salt = std::env::var(TEST_SALT).unwrap_or_default();
    let base = format!("{DIGEST}+{}", super::digest(&super::TSROWS));
    if salt.is_empty() { base } else { format!("{base}+{salt}") }
}

#[cfg(test)]
#[path = "node_rows_tests.rs"]
mod tests;
