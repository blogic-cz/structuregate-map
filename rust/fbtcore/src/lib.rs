//! fbtcore — the C ABI StructureGate links against. Its doors are few: the whole command (`cli::fbt_main`),
//! the SQL the links ask of the map (`rows::sqlrun`), and freeing what either returned (`abi`).
//!
//! Built as a STATIC library and linked into the NativeAOT exe, so a consumer still
//! receives two files. Nothing here allocates across the boundary without saying who
//! frees it: every string this library returns is freed by `fbt_string_free`, and
//! nothing else.
//!
//! ## Panics never cross the boundary
//!
//! `extern "C"` aborts the process if a panic reaches it, and this library lives
//! inside somebody else's build. Every entry point therefore runs its body inside
//! `catch_unwind` and turns a panic into an error the caller can report. That is why
//! the release profile does not set `panic = "abort"`.

/// Every rust allocation, and only rust's: nothing allocated here is freed by the host (`fbt_string_free` is ours).
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod abi;
mod atlas;
mod cli;
mod count;
mod csproj;
mod cssmap;
mod embedded;
mod facts;
mod gate;
mod gomap;
mod graph;
mod hosts;
mod mapper;
mod mdmap;
mod query;
mod tree;
mod rows;
mod rsmap;
mod session;
mod sources;
mod trace;
mod view;

pub(crate) use abi::{in_string, out_string};
