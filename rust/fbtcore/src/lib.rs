//! fbtcore — the C ABI StructureGate links against. Its doors are few: the whole command (`cli::fbt_main`),
//! the SQL the links ask of the map (`rows::sqlrun`), a PostgreSQL file's rows (`sqlpg::fbt_sql_parse`), and
//! freeing what any of them returned (`abi`).
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
/// A `metrics` build counts them on the way (`trace::metrics::Counting`).
#[cfg(not(feature = "metrics"))]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// FREED MEMORY BACK TO THE SYSTEM, after a large structure was dropped: mimalloc keeps the pages it freed for the next
/// allocation, and the process went on holding gigabytes nothing used - after `Store::release` most of what the store
/// held. `mi_collect(true)` frees this thread's empty pages and purges the arenas now. Declared by hand: the symbol is
/// in the mimalloc the allocator already links (`libmimalloc-sys` declares it only under a feature), so nothing is added.
pub(crate) fn give_back() {
    unsafe extern "C" {
        fn mi_collect(force: bool);
    }
    // SAFETY: mimalloc's own entry point, callable from any thread at any time; it touches only the allocator's state.
    unsafe { mi_collect(true) }
}

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
mod slice;
mod sources;
mod sqlpg;
mod trace;
mod view;

pub(crate) use abi::{in_string, out_string};
