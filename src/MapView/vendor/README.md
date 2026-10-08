# Vendored libraries of the map view

Compiled into the exe by `rust/fbtcore/src/view/mod.rs`, so `--map-view` writes a page that opens offline. Taken
unchanged from npm; to update, replace the file and say so in the commit.

| file | package | version | licence |
|---|---|---|---|
| `sigma.min.js` | [sigma](https://www.sigmajs.org) (`dist/sigma.min.js`) | 3.0.3 | MIT |
| `graphology.umd.min.js` | [graphology](https://graphology.github.io) (`dist/graphology.umd.min.js`) | 0.26.0 | MIT |
| `graphology-library.min.js` | graphology-library (`dist/graphology-library.min.js`) - ForceAtlas2, Louvain | 0.8.0 | MIT |
