# libpg_query, vendored

PostgreSQL's own parser as a C library: the PostgreSQL 17 grammar, scanner and PL/pgSQL compiler, from
[pganalyze/libpg_query](https://github.com/pganalyze/libpg_query).

- **Tag** `17-6.2.5` (PostgreSQL 17.7, libpg_query 6.2.5), **commit** `c2c27b1e5b13584b594e5dee082f9d007b5e721b`.
- **License**: BSD 3-Clause for libpg_query (`LICENSE`), the PostgreSQL License for the PostgreSQL sources
  (`src/postgres/COPYRIGHT`); `vendor/protobuf-c` and `vendor/xxhash` carry their own (BSD 2-Clause) in their headers.

**Only what the build compiles is kept** - the list `rust/fbtcore/build.rs` compiles, which is upstream's
`Makefile` / `Makefile.msvc` list: `src/*.c`, `src/postgres/*.c`, `vendor/protobuf-c/protobuf-c.c`,
`vendor/xxhash/xxhash.c`, `protobuf/pg_query.pb-c.c`, and every header under `src/`, `vendor/` and the root.
Left out: `test/`, `examples/`, `scripts/`, `patches/`, `srcdata/`, `testdata/`, the `.proto` and its generated
C++/Ruby, `src/pg_query_outfuncs_protobuf_cpp.cc`, the Makefiles. The files are upstream's, unchanged.

To move to another tag: fetch it, copy the same set over this folder, keep this file, rebuild, and run the SQL suites
(`tests/deep/sql/`) - the JSON the half reads is the tag's own node structs, so a renamed field shows up there.
