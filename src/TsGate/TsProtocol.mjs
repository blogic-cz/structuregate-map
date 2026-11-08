/**
 * ONE PROTOCOL RECORD, for every TypeScript script the gate runs: the file map's (`TsGate.Map.mjs`), the plain half's
 * (`TsPlain.mjs`) and the Angular half's (`TsRunIo.mjs` re-exports it). A field carrying `|`, a newline or a bare
 * carriage return would split into the wrong slot or the wrong line on the rust side, so all three are folded here -
 * three copies of this had drifted, and the file map's let a `\r` through.
 *
 * A LEAF: it imports nothing, so every set in `rust/fbtcore/src/embedded/mod.rs` that stages one of its importers can
 * stage it beside it.
 */
export function emit(...fields) {
    const parts = fields.map((field) => String(field).split('|').join('/').split('\r').join(' ').split('\n').join(' '));
    process.stdout.write(parts.join('|') + '\n');
}

/** A LINE THE GATE PRINTS UNDER THE HALF'S NAME - what it did, skipped or could not resolve. The tag is spelled here once. */
export const note = (...fields) => emit('MAP-NOTE', ...fields);

/** THE HALF CANNOT GO ON: the run fails, and the rust side keeps no carried rows from it (`mapper/script.rs`). */
export const fatal = (...fields) => emit('MAP-FATAL', ...fields);
