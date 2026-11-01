<#
    WHAT THE TREE'S HASH READ, SAID IN THE TIMING NOTE (#144, `mapper/deep/refresh.rs`): beside `hash the tree` the note
    names the files read from disk, what they held and every entry the snapshot names. The first run reads every file,
    a large one `--ext` never maps included; after a one-file edit only that file is read again, so the large one costs
    nothing while its size and mtime stay put.
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
