<#
    WHAT THE TREE'S HASH READ, SAID IN THE TIMING NOTE (#144, `mapper/deep/refresh.rs`): beside `hash the tree` the note
    names the files read from disk, what they held and every entry the snapshot names. The first run reads every file,
    a large one `--ext` never maps included; after a one-file edit only that file is read again, so the large one costs
    nothing while its size and mtime stay put.
    Folders whose names differ only in case keep the snapshot off Windows.
    Its helpers are its own - `-Only DeepHashRead` runs this suite alone.
#>

Test-Case 'deephashread: the timing note says what the hash read, and an edit reads only the edited file' {
    $tree = Use-Tree @{
        'src/lib.rs' = "pub mod alpha;`n"
        'src/alpha.rs' = "pub fn alpha() -> u32 { 1 }`n"
    }
    New-Item -ItemType Directory -Force (Join-Path $tree 'blobs') | Out-Null
    [System.IO.File]::WriteAllBytes((Join-Path $tree 'blobs/model.bin'), [byte[]]::new(3MB))
    $db = Join-Path $tree 'map.sqlite'
    $first = Invoke-Gate --root $tree --ext .rs --map-sqlite $db
    Assert-Exit $first 0
    Assert-Line $first '(read 3 file(s), 3.0 MB, of '
    [System.IO.File]::AppendAllText((Join-Path $tree 'src/alpha.rs'), "// edited`n")
    $edited = Invoke-Gate --root $tree --ext .rs --map-sqlite $db
    Assert-Exit $edited 0
    Assert-Line $edited '(read 1 file(s), 38 B, of '
}

# TWO FOLDERS WHOSE NAMES DIFFER ONLY IN CASE (`fbt::entry::fold`): off Windows they are two, and one key for both failed the
# snapshot's store - every lookup missed and every half read every file on every run, said only as `of 0 entries`. Windows
# cannot hold both, so the case has nothing to ask there.
Test-Case 'deephashread: folders that differ only in case keep the snapshot, and an edit reads one file' {
    if ($IsWindows) { return }
    $tree = Use-Tree @{
        'src/Axa/provider.rs' = "pub fn provider() -> u32 { 1 }`n"
        'src/AXA/utils.rs' = "pub fn utils() -> u32 { 2 }`n"
    }
    $db = Join-Path $tree 'map.sqlite'
    $first = Invoke-Gate --root $tree --ext .rs --map-sqlite $db
    Assert-Exit $first 0
    Assert-NoLine $first 'could not snapshot'
    Assert-Line $first '(read 2 file(s), '
    [System.IO.File]::AppendAllText((Join-Path $tree 'src/AXA/utils.rs'), "// edited`n")
    $edited = Invoke-Gate --root $tree --ext .rs --map-sqlite $db
    Assert-Exit $edited 0
    Assert-Line $edited '(read 1 file(s), 38 B, of '
}

# WHAT GIT IGNORES AND WHAT NO TREE MAPS ARE NO PART OF THE SNAPSHOT (#148, `mapper/deep/hashes.rs`): an ignored folder and
# `node_modules` were walked and their every file stored, so a tree of two source files hashed for seconds.
Test-Case 'deephashread: the snapshot leaves out what .gitignore names and node_modules' {
    $tree = Use-Tree @{
        'src/lib.rs' = "pub fn alpha() -> u32 { 1 }`n"
        '.gitignore' = "cache/`n"
    }
    foreach ($folder in 'cache', 'node_modules/demo') {
        New-Item -ItemType Directory -Force (Join-Path $tree $folder) | Out-Null
        [System.IO.File]::WriteAllBytes((Join-Path $tree "$folder/model.bin"), [byte[]]::new(1MB))
    }
    $db = Join-Path $tree 'map.sqlite'
    $first = Invoke-Gate --root $tree --ext .rs --map-sqlite $db
    Assert-Exit $first 0
    Assert-Line $first '(read 2 file(s), 35 B, of '
}
