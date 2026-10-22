<#
    The Angular half's `handlers` and `raises` rows, and the `try` row of its `branches`: a catch clause in
    the python half's columns, a throw under the branch it sits in, and a try that no statement is attributed
    to - so the closure reads a try block as it did before. See `collectTry` and `emitThrow` in
    `src/TsRows/TsDecls/TsBody.mjs` and `src/TsRows/TsDecls/TsCatch.mjs`. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../TsRows.Helpers.ps1')

# Lines as numbered in the cases: the tries on 4 and 12, the catches on 6 and 12, the `if` on 13. The
# assignment inside the `if` is there so `assignments` has a `branch` column at all.
$script:TsRowsHandlersGuard = "export class Guard {`n  state = '';`n" +
    "  run(log: { warn(m: string, e: unknown): void }, a: () => void): void {`n" +
    "    try {`n      this.state = 'go';`n    } catch (e) { // best effort`n      log.warn('x', e);`n      throw e;`n" +
    "    } finally {`n      this.state = 'done';`n    }`n    try { a(); } catch { /* swallow */ }`n" +
    "    if (this.state) { this.state = 'bad'; throw new Error('bad'); }`n  }`n}`n"

if ($script:TsRowsModules -and $script:TsRowsPython) {

Test-Case 'tsrows: a catch clause is a handlers row of the Angular half' {
    $made = New-TsRowsDb (New-TsRowsWorkspace @{ 'apps/shop/src/guard.ts' = $script:TsRowsHandlersGuard }) -Shared
    Assert-Exit $made.Result 0
    # Digits: bare, name_read, passes, raises, reraises, finally.
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'h=' || line || ':' || try_line || ':' || name || ':' || bare || name_read || passes" +
        " || raises || reraises || finally || ':' || comment || ':' || $(Get-NodeRootListSql 'handlers__calls' 'handlers.id') || '|' || half FROM handlers ORDER BY line")
    Assert-Line $r 'h=6:4:e:110111:// best effort:["log.warn"]|typescript'
    Assert-Line $r 'h=12:12::101000:/* swallow */:[]|typescript'
    Assert-Line (Invoke-TsRowsQ $made.Db "SELECT 'unowned=' || count(*) FROM handlers WHERE member IS NULL") 'unowned=0'
}

Test-Case 'tsrows: a try is a branch no statement sits in, and a throw is a raises row under its branch' {
    $made = New-TsRowsDb (New-TsRowsWorkspace @{ 'apps/shop/src/guard.ts' = $script:TsRowsHandlersGuard }) -Shared
    Assert-Exit $made.Result 0
    $b = Invoke-TsRowsQ $made.Db "SELECT 'b=' || line || ':' || sense || ':' || coalesce(parent, '-') FROM branches ORDER BY line"
    Assert-Line $b 'b=4:try:-'
    Assert-Line $b 'b=12:try:-'
    Assert-Line $b 'b=13:then:-'
    # THE GUARD OF THE CLOSURE: a statement inside a try block is as unconditional as it was.
    $a = Invoke-TsRowsQ $made.Db ("SELECT 'a=' || line || ':' || target || ':' || coalesce(branch, '-') FROM assignments " +
        "WHERE target = 'state' ORDER BY line")
    Assert-Line $a 'a=5:state:-'
    Assert-Line $a 'a=10:state:-'
    Assert-Line $a 'a=13:state:br:'
    $t = Invoke-TsRowsQ $made.Db ("SELECT 'r=' || line || ':' || name || ':' || (branch IS NOT NULL) || ':' || source " +
        "FROM raises ORDER BY line")
    Assert-Line $t 'r=8:e:0:e'
    Assert-Line $t "r=13:Error:1:new Error('bad')"
}

# EVERY COMMENT OF A PARSED FILE IS A ROW, ONCE (rows 78, `TsText.mjs::collectComments`): a bare scanner has no grammar, so a regex holding a backtick opened a template that swallowed every comment up to the
# next backtick - and the file's last ones. A comment in a call chain, after the last member and after a template is a row as one on a declaration is. Control: the comment before it all, and a total with no duplicate.
$script:TsRowsCommentsNote = @'
// HEAD note
declare const api: any;
export class Alpha {
  run() {
    return api.get('/x')
      // Verify OTP - POST Sign
      .pipe((r: number) => r);
  }
  tick = /`/; // regex with a quote
  // ARCHITECTURE END
}
const tpl = `a ${1} b`; // after template
// FINAL note
'@

Test-Case 'tsrows: a comment in a call chain, after the last member and after a regex or template is a comments row, once' {
    $made = New-TsRowsDb (New-TsRowsWorkspace @{ 'apps/shop/src/note.ts' = $script:TsRowsCommentsNote }) -Shared
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'c=' || c.line || ':' || c.text AS r FROM comments c JOIN files f ON f.id = c.file WHERE f.path LIKE '%note.ts' ORDER BY c.line")
    Assert-Line $r 'c=1:// HEAD note'
    Assert-Line $r 'c=6:// Verify OTP - POST Sign'
    Assert-Line $r 'c=9:// regex with a quote'
    Assert-Line $r 'c=10:// ARCHITECTURE END'
    Assert-Line $r 'c=12:// after template'
    Assert-Line $r 'c=13:// FINAL note'
    Assert-Line (Invoke-TsRowsQ $made.Db "SELECT 'n=' || count(*) AS r FROM comments c JOIN files f ON f.id = c.file WHERE f.path LIKE '%note.ts'") 'n=6'
    # A comment keeps the declaration it sits in: the one after the last member is the class's.
    Assert-Line (Invoke-TsRowsQ $made.Db "SELECT 'ctx=' || c.context AS r FROM comments c WHERE c.text LIKE '%ARCHITECTURE END'") 'ctx=Alpha'
}

}
