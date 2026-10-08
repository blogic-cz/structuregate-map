/**
 * What the Angular half reads and writes around a run: its protocol lines, its arguments, the carried rows and the payload.
 *
 * Moved out of `TsMap.mjs` so that file stays under the line ceiling; it imports FLAT, because every
 * staged script lands in one folder.
 */

import { readFileSync, openSync, writeSync, closeSync, readSync } from 'node:fs';
import { StringDecoder } from 'node:string_decoder';

export function emit(...fields) {
  const parts = fields.map((f) => String(f).split('|').join('/').split('\r').join(' ')
    .split('\n').join(' '));
  process.stdout.write(parts.join('|') + '\n');
}

export function parseArgs(argv) {
  const args = { root: '', rows: '', state: '', fe: '', config: '', db: '', html: '', plan: '',
    hashes: '', lines: '', carry: '', everyHop: false, nodeModules: [], skipDirs: ['node_modules'] };
  for (let i = 0; i < argv.length; i++) {
    const flag = argv[i];
    const value = argv[i + 1];
    if (flag === '--root') { args.root = value; i++; }
    else if (flag === '--rows') { args.rows = value; i++; }
    else if (flag === '--state') { args.state = value; i++; }
    else if (flag === '--fe') { args.fe = value; i++; }
    else if (flag === '--config') { args.config = value; i++; }
    // `--skip-dir` IS A FOLDER NAME THE GATE SKIPS, repeatable - the same `--skip` every other half walks by.
    else if (flag === '--node-modules' || flag === '--skip-dir') { args[flag === '--skip-dir' ? 'skipDirs' : 'nodeModules'].push(value); i++; }
    else if (flag === '--db') { args.db = value; i++; }
    else if (flag === '--html') { args.html = value; i++; }
    else if (flag === '--plan') { args.plan = value; i++; }
    // THE PLAN THIS RUN FOLLOWS, whose hashes it takes instead of walking the tree a second time.
    else if (flag === '--hashes') { args.hashes = value; i++; }
    // `{path: [sha, lines]}` as the database holds them - see `inventory`'s `known`.
    else if (flag === '--lines') { args.lines = value; i++; }
    // THE ROWS OF EVERY FILE THIS RUN IS NOT RE-EXTRACTING, written by the storing half - see its
    // `carry_over`. Its presence is not what makes a run partial: this half decides that from the
    // same hashes it decides `--plan` from, and ignores the file when the answer is a full run.
    else if (flag === '--carry') { args.carry = value; i++; }
    // THE RETRY OF A RUN THAT STOPPED SHORT and found it could not (`MAP-RETRY`): every hop of dependents.
    else if (flag === '--every-hop') { args.everyHop = true; }
  }
  return args;
}

/**
 * The payload, written WITHOUT EVER BUILDING IT AS ONE STRING.
 *
 * `JSON.stringify(payload)` produced a single string of hundreds of millions of characters on a
 * large workspace. V8 refuses one longer than 536 870 888 - `Invalid string length`, which no heap setting
 * moves - so this half was a modest growth in a workspace away from being unable to write its own rows at all.
 *
 * THE BYTES ARE THE SAME BYTES. Keys go out in insertion order with no spaces, exactly as
 * `JSON.stringify` emits them; only `tables` is walked row by row, because that is where the size is.
 */
/**
 * The carried rows, read WITHOUT EVER HOLDING THE FILE AS ONE STRING.
 *
 * `JSON.parse(readFileSync(file, 'utf8'))` makes two strings the size of the file, and this one is
 * hundreds of MB on a real tree - most of V8's hard ceiling of 536 870 888 characters. That is the nearest
 * wall in this half, and `Invalid string length` is not something a heap flag moves.
 *
 * One line is one row - `{"t": <table>, "r": <row>}` - with a final `{"claims": {...}}`. Written by
 * `rows::carry::carry_over`; the shape is private between the two.
 */
export function readCarry(file) {
  const tables = {};
  let claims = {};
  let fd;
  try {
    fd = openSync(file, 'r');
  } catch {
    return null;
  }
  // A CHUNK CAN SPLIT A CHARACTER. `StringDecoder` holds the tail of a multi-byte sequence until the
  // rest of it arrives; `Buffer.toString` on the boundary would put a replacement character in a row.
  const decoder = new StringDecoder('utf8');
  const buffer = Buffer.allocUnsafe(1 << 22);
  let held = '';
  const take = (line) => {
    if (!line) return;
    const one = JSON.parse(line);
    if (one.claims) { claims = one.claims; return; }
    (tables[one.t] ??= []).push(one.r);
  };
  try {
    for (;;) {
      const read = readSync(fd, buffer, 0, buffer.length, null);
      if (!read) break;
      held += decoder.write(buffer.subarray(0, read));
      let at;
      while ((at = held.indexOf('\n')) >= 0) {
        take(held.slice(0, at));
        held = held.slice(at + 1);
      }
    }
    held += decoder.end();
    take(held.trim());
  } finally {
    closeSync(fd);
  }
  return { tables, claims };
}

export function writePayload(file, payload) {
  const fd = openSync(file, 'w');
  let held = [];
  let size = 0;
  const put = (text) => {
    held.push(text);
    size += text.length;
    // Four megabytes at a time: bounded, and still one write for thousands of rows.
    if (size >= (1 << 22)) { writeSync(fd, held.join('')); held = []; size = 0; }
  };
  try {
    put('{');
    let firstKey = true;
    for (const [key, value] of Object.entries(payload)) {
      // `JSON.stringify` drops a key whose value is undefined; so does this.
      if (value === undefined) continue;
      if (!firstKey) put(',');
      firstKey = false;
      put(JSON.stringify(key));
      put(':');
      if (key !== 'tables') { put(JSON.stringify(value)); continue; }
      put('{');
      let firstTable = true;
      for (const [name, rows] of Object.entries(value)) {
        if (rows === undefined) continue;
        if (!firstTable) put(',');
        firstTable = false;
        put(JSON.stringify(name));
        put(':[');
        for (let i = 0; i < rows.length; i++) {
          if (i) put(',');
          // An undefined ELEMENT is `null` in JSON, which is what `JSON.stringify` writes.
          put(JSON.stringify(rows[i]) ?? 'null');
        }
        put(']');
      }
      put('}');
    }
    put('}');
    if (held.length) writeSync(fd, held.join(''));
  } finally {
    closeSync(fd);
  }
}

export function readJson(file, fallback) {
  if (!file) return fallback;
  try {
    return JSON.parse(readFileSync(file, 'utf8'));
  } catch {
    return fallback;
  }
}
