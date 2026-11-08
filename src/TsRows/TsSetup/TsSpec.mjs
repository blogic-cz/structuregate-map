/**
 * WHAT THE PAYLOAD SAYS ABOUT ITS OWN ROWS: the spec (id scheme, joins, file routes, structured and boolean columns) and the
 * provenance block. Both are read off the finished store, so `TsMap.mjs` calls them last, after every pass.
 */
import path from 'node:path';

import { names } from './TsConfig.mjs';
import { columnKinds, deriveJoins, sortedKeys } from '../TsDerive/TsSpecJoins.mjs';
import { deriveAnchors } from '../TsDerive/TsSpecAnchors.mjs';
import { REWRITTEN } from '../TsStore.mjs';
import { note, revisionOf } from './TsRunIo.mjs';
import { timed } from './TsMetrics.mjs';

/**
 * THE MAP STATES ITS OWN FOREIGN KEYS AND ITS OWN FILE ROUTE. SQLite records neither, and a consumer
 * that wants either had to write its own chain of `file` / `class` / `member` / `template` / `node` -
 * which several of them did, each a slightly different subset, so two readers disagreed about the same row
 * and the one resolving fewer rows looked exactly like a map that lacked them. Derived LAST, because
 * both are read off the finished rows rather than declared anywhere.
 *
 * `replacedRows` is null on a full run; `state` is what the last run recorded.
 */
export function deriveSpec(store, state, replacedRows) {
  const tableNames = Object.keys(store.tables).sort();
  const idScheme = store.idScheme();
  // A TABLE NO ROW OF WHICH MOVED is the table the last spec was derived from: no row of a file read again, none made by this
  // run, no rollup's and none a pass rewrites (`REWRITTEN`, which `verifyUntouched` lets through) - and the same id scheme, which
  // names every join's targets. Its joins, structured and boolean columns and identity key are the last spec's; only the rest is
  // scanned, which on a partial run is a few tables of every row of the map.
  const unmoved = new Set(replacedRows === null || JSON.stringify(state.id_scheme ?? null) !== JSON.stringify(idScheme) ? []
    : tableNames.filter((t) => !(replacedRows[t] > 0) && !REWRITTEN.includes(t) && !store.rollupTables.has(t)
      && store.table(t).every((row) => store.hydrated.has(row))));
  const moved = tableNames.filter((t) => !unmoved.has(t));
  if (replacedRows !== null) note(`the typescript half kept the spec of ${unmoved.size} of ${tableNames.length} table(s) no row of which moved`);
  const keptOf = (spec) => Object.fromEntries(Object.entries(spec ?? {}).filter(([t]) => unmoved.has(t)));
  const kinds = timed('typescript: columnKinds', '', () => columnKinds(store, moved));
  // IN THE ORDER A WHOLE DERIVATION WRITES THEM: column kinds by table name as `tableNames` sorts, joins as `deriveJoins` sorts.
  const byTable = (o) => Object.fromEntries(tableNames.filter((t) => o[t] !== undefined).map((t) => [t, o[t]]));
  const jsonColumns = byTable({ ...kinds.jsonColumns, ...keptOf(state.json_columns) });
  const boolColumns = byTable({ ...kinds.boolColumns, ...keptOf(state.bool_columns) });
  const joins = timed('typescript: deriveJoins', '', () => sortedKeys({ ...deriveJoins(store, idScheme, moved),
    ...Object.fromEntries(Object.entries(state.joins ?? {}).filter(([key]) => unmoved.has(key.slice(0, key.lastIndexOf('.'))))) }));
  const { anchors, unanchored } = timed('typescript: deriveAnchors', '', () => deriveAnchors(store, idScheme, joins, tableNames));
  note(`the typescript half proved a file route for ${Object.keys(anchors).length} table(s) `
    + `over ${Object.keys(joins).length} join(s)`
    + (unanchored.length ? `, and none for ${unanchored.join(', ')}` : ''));
  return { tableNames, idScheme, unmoved, jsonColumns, boolColumns, joins, anchors, unanchored };
}

/**
 * WHAT THIS MAP IS, for a consumer that puts it in a derived artifact's provenance block. The tool
 * being reproduced publishes these three in `index.json`, and a downstream consumer writes them into every
 * artifact it derives, so a reader can tell which extraction an answer came from. SQLite records none
 * of it, so the half that knows states it - beside the join spec, under the same key.
 */
export function provenanceOf({ setup, fe, tsVersion, ngVersion, found, warning, templatePass, config }) {
  return {
    setup,
    generated_at_ms: Date.now(),
    // WHICH SOURCE THIS MAP IS OF, recorded at EXTRACTION time. A reader asked later would report the
    // revision it is standing on, which is not the one the rows describe. `dirty` stays because a map
    // built from an uncommitted tree is not the commit it names.
    source_revision: revisionOf(fe),
    toolchain: {
      typescript: tsVersion, angular_compiler: ngVersion, node: process.version,
      tier: found.tier, warning: warning ?? null,
    },
    options: {
      ...templatePass.options,
      i18n_calls: names(config, 'i18nCalls'),
      i18n_dictionary_calls: names(config, 'i18nDictionaryCalls'),
      // THE INPUTS A FORM CONTROL'S LABEL IS BOUND UNDER (#155, `control_labels`): `labelInputs`, else `label` and `title`.
      label_inputs: Array.isArray(config.data?.labelInputs) ? names(config, 'labelInputs') : ['label', 'title'],
      // WHAT SHOWS AN OPTION'S PROPERTY AS HELP, not as the option (#156, `control_options.shown_as`): an element `hintTags` names, an input `hintInputs` does.
      hint_tags: Array.isArray(config.data?.hintTags) ? names(config, 'hintTags') : ['mat-hint'],
      hint_inputs: Array.isArray(config.data?.hintInputs) ? names(config, 'hintInputs') : ['title', 'matTooltip'],
      locales: names(config, 'locales'),
      feature_checks: names(config, 'featureChecks'),
      feature_enum: config.data?.featureEnum ?? '',
      config: config.file,
      // WHERE THIS PROJECT KEEPS WHAT IT HAS ALREADY WORKED OUT ABOUT ITSELF. Nothing in the extraction
      // reads it - it is a pointer the atlas prints, so the records stay findable from the one document
      // a reader opens.
      // ABSOLUTE, because it is a pointer a reader follows from a document that may be read anywhere.
      // Resolved against the CONFIG's own directory, like every other path that file declares.
      project_docs: typeof config.data?.projectDocs === 'string'
        ? path.resolve(config.dir, config.data.projectDocs) : null,
    },
  };
}
