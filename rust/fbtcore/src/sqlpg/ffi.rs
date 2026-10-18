//! LIBPG_QUERY'S C DOORS, declared by hand (`vendor/libpg_query/pg_query.h`, compiled by `build.rs`): no bindgen, so
//! the build needs a C compiler and nothing else. Each call answers in rust types and frees what the library returned
//! before it returns; nothing the library allocated outlives the call.
//!
//! The library keeps its state per THREAD (`__thread`) and frees a thread's memory context when the thread ends (a
//! `pthread` key it registers on the thread's first call) - so every call is made from the one parse thread (`mod.rs`).

use serde::Deserialize;
use serde_json::Value;
use std::ffi::{c_char, c_int, CStr, CString};

#[repr(C)]
struct PgQueryError {
    message: *mut c_char,
    funcname: *mut c_char,
    filename: *mut c_char,
    lineno: c_int,
    cursorpos: c_int,
    context: *mut c_char,
}

#[repr(C)]
struct PgQueryParseResult {
    parse_tree: *mut c_char,
    stderr_buffer: *mut c_char,
    error: *mut PgQueryError,
}

#[repr(C)]
struct PgQueryPlpgsqlParseResult {
    plpgsql_funcs: *mut c_char,
    error: *mut PgQueryError,
}

#[repr(C)]
struct PgQueryProtobuf {
    len: usize,
    data: *mut c_char,
}

#[repr(C)]
struct PgQueryScanResult {
    pbuf: PgQueryProtobuf,
    stderr_buffer: *mut c_char,
    error: *mut PgQueryError,
}

unsafe extern "C" {
    fn pg_query_parse_opts(input: *const c_char, parser_options: c_int) -> PgQueryParseResult;
    fn pg_query_parse_plpgsql(input: *const c_char) -> PgQueryPlpgsqlParseResult;
    fn pg_query_scan(input: *const c_char) -> PgQueryScanResult;
    fn pg_query_free_parse_result(result: PgQueryParseResult);
    fn pg_query_free_plpgsql_parse_result(result: PgQueryPlpgsqlParseResult);
    fn pg_query_free_scan_result(result: PgQueryScanResult);
}

/// What the parser refused: its message and where, a BYTE offset into the text it was given (the library counts a
/// 1-based character; 0 when it names no place).
#[derive(Debug, Clone)]
pub struct Refused {
    pub message: String,
    pub at: Option<usize>,
    /// Where the library was when it refused, in words (PL/pgSQL: `... near line 3`).
    pub context: String,
}

/// How a text is parsed - libpg_query's `PgQueryParseMode`: a whole statement, or what PL/pgSQL hands the parser for an
/// expression or an assignment.
pub const MODE_STATEMENT: i32 = 0;

/// One token of the scanner: its byte range and whether it is an identifier or a keyword.
#[derive(Debug, Clone)]
pub struct Token {
    pub start: usize,
    pub end: usize,
    pub kind: i64,
    pub keyword: bool,
}

/// The scanner's token numbers this half asks about (`protobuf/pg_query.proto`, `enum Token`).
pub const IDENT: i64 = 258;
pub const SQL_COMMENT: i64 = 275;
pub const C_COMMENT: i64 = 276;

fn c_text(text: &str) -> Result<CString, Refused> {
    CString::new(text).map_err(|e| Refused { message: format!("the text holds a NUL byte at {}", e.nul_position()), at: Some(e.nul_position()), context: String::new() })
}

unsafe fn owned(ptr: *const c_char) -> String {
    if ptr.is_null() { String::new() } else { unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned() }
}

unsafe fn refused(text: &str, error: *const PgQueryError) -> Refused {
    let error = unsafe { &*error };
    let at = usize::try_from(error.cursorpos).ok().filter(|&c| c > 0).map(|c| text.char_indices().nth(c - 1).map_or(text.len(), |(i, _)| i));
    Refused { message: unsafe { owned(error.message) }, at, context: unsafe { owned(error.context) } }
}

/// The library's JSON, read without serde_json's recursion limit: a tree is as deep as its statement. The parse runs on
/// a thread with a stack sized for it (`super::STACK`).
fn json(raw: &str) -> Value {
    let mut reader = serde_json::Deserializer::from_str(raw);
    reader.disable_recursion_limit();
    Value::deserialize(&mut reader).unwrap_or(Value::Null)
}

/// One text parsed in `mode`: `{"version", "stmts": [{"stmt", "stmt_location", "stmt_len"}]}`.
pub fn parse(text: &str, mode: i32) -> Result<Value, Refused> {
    let input = c_text(text)?;
    let result = unsafe { pg_query_parse_opts(input.as_ptr(), mode) };
    let answer = if result.error.is_null() { Ok(json(&unsafe { owned(result.parse_tree) })) } else { Err(unsafe { refused(text, result.error) }) };
    unsafe { pg_query_free_parse_result(result) };
    answer
}

/// A CREATE FUNCTION or DO statement's PL/pgSQL body, compiled the way PostgreSQL's PL/pgSQL compiles it: a list of
/// `PLpgSQL_function`. The library's position here is a line of the body (`lineno` in the message), not a cursor.
pub fn parse_plpgsql(text: &str) -> Result<Value, Refused> {
    let input = c_text(text)?;
    let result = unsafe { pg_query_parse_plpgsql(input.as_ptr()) };
    let answer = if result.error.is_null() { Ok(json(&unsafe { owned(result.plpgsql_funcs) })) } else { Err(unsafe { refused(text, result.error) }) };
    unsafe { pg_query_free_plpgsql_parse_result(result) };
    answer
}

/// Every token PostgreSQL's scanner reads in a text, comments included.
pub fn scan(text: &str) -> Result<Vec<Token>, Refused> {
    let input = c_text(text)?;
    let result = unsafe { pg_query_scan(input.as_ptr()) };
    let answer = if result.error.is_null() {
        let bytes: &[u8] = if result.pbuf.data.is_null() { &[] } else { unsafe { std::slice::from_raw_parts(result.pbuf.data as *const u8, result.pbuf.len) } };
        Ok(super::scan::decode(bytes))
    } else {
        Err(unsafe { refused(text, result.error) })
    };
    unsafe { pg_query_free_scan_result(result) };
    answer
}
