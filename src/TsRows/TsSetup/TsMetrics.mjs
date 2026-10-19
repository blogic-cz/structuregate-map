/**
 * THE TYPESCRIPT HALF'S METRICS - per program, per pass - ON ONLY WHEN RUST HANDS `SG_METRICS` A FILE, which a rust build
 * with the `metrics` feature does (`dotnet build -p:Metrics=true`, `rust/fbtcore/src/trace/metrics.rs`). A script is not
 * compiled, so the calls stay; off, each is one test of a constant `false` and the item is never built.
 *
 * Tallied here and written ONCE, as the process exits, as the JSON array rust merges into the run's trace: per name how
 * many, how long in all, the slowest and which it was, and every item slower than `SG_METRICS_SLOW_MS` (50 by
 * default). TIMING ONLY: nothing here may change a row.
 */
import { writeFileSync } from 'node:fs';

const FILE = process.env.SG_METRICS || '';
export const METRICS = FILE !== '';
const SLOW_MS = Number(process.env.SG_METRICS_SLOW_MS || 50);
const TALLIES = new Map();

/** `fn()`, timed as one item of `name` when metrics are on. `item` is a string or a function that builds one. */
export function timed(name, item, fn) {
  if (!METRICS) return fn();
  const started = performance.now();
  try {
    return fn();
  } finally {
    record(name, typeof item === 'function' ? item() : item, performance.now() - started);
  }
}

function record(name, item, ms) {
  let tally = TALLIES.get(name);
  if (!tally) TALLIES.set(name, tally = { name, count: 0, total_ms: 0, max_ms: 0, max_item: '', slow: [] });
  tally.count += 1;
  tally.total_ms += ms;
  if (ms > tally.max_ms) { tally.max_ms = ms; tally.max_item = String(item); }
  if (ms >= SLOW_MS) tally.slow.push([String(item), ms]);
}

if (METRICS) {
  process.on('exit', () => {
    try { writeFileSync(FILE, JSON.stringify([...TALLIES.values()])); } catch { /* the trace is never worth a failed run */ }
  });
}
