<#
    PsRows.ps1 - the PowerShell half of the DEEP map (`--map-sqlite`): every row a `.ps1`/`.psm1`/`.psd1` file
    contributes, as one payload the exe stores (`rust/fbtcore/src/mapper/deep/payload.rs`), `files.lang = 'powershell'`.

    THE SAME TABLES AS THE PYTHON AND PLAIN TYPESCRIPT HALVES wherever the meaning is the same - `files`, `functions`,
    `classes`, `imports`, `calls`, `branches`, `handlers`, `raises`, `assignments`, `string_literals`,
    `number_literals`, `expressions`, `regexes` - so one query reads every language, told apart by `files.lang`.

    ONE FINGERPRINT WITH THE FILE MAP (`PsShape.ps1`): `functions.body_shape` and `expressions.shape` are the digests
    `PsMap.ps1` prints as MAP-BODY and MAP-EXPR, over the same nodes and by the same rule - a function declared inside
    a scriptblock literal has no body digest there, so it has none here.

    SCOPE IS RECORDED, NOT INFERRED: a row says which class and which named function it sits in (`cls`, `func`).

    SYNTACTIC: a command is named as written (`callee`), never bound - PowerShell resolves a name at run time, and a
    cmdlet is as much a command as a function of the tree. A REGEX is read where the code builds one: a match,
    replace or split operator, a `switch -Regex` clause, a cast to the regex type or a static call on it; its pattern
    is known only from a literal.

    Contract with the caller (structuregate.exe) - the python half's, word for word:
      in   --list-file <path>  UTF-8, one file per line: <relative-path><TAB><absolute-path>
           --root <dir>        the tree being mapped
           --state <path>      what the database holds for this half (`rows::state`)
           --rows <path>       where the payload goes
           --hashes <path>     {rel: the tree map's content hash}: a file handed so is not READ unless it is parsed
           --reset             the retry: drop every file this half records and write them again
      out  the payload at --rows; MAP-ERROR, MAP-READ and MAP-DONE on stdout. Exit 0 - the caller owns the verdict.

    PARSED AND RUN BY 5.1 as well as 7. NO REGEX, ANYWHERE - the build refuses one over this folder.
#>
using namespace System.Management.Automation.Language

$ErrorActionPreference = 'Stop'
. ([System.IO.Path]::Combine($PSScriptRoot, 'PsShape.ps1'))

# Part of every file's sha: changing what a file's rows hold changes this, and every file is read again.
$script:RowsVersion = 'ps-rows 1'
# The variables an expression reads, asked in .NET (`$script:EveryNode`'s trick): a scriptblock per node was a third of the walk.
$script:Variables = [Delegate]::CreateDelegate([Func[Ast, bool]], [VariableExpressionAst], [Type].GetMethod('IsInstanceOfType', [Type[]]@([object])))
$script:Counters = @{}
$script:Tables = [ordered]@{}
# The type names a regex is built from.
$script:RegexTypes = @('regex', 'System.Text.RegularExpressions.Regex', 'Text.RegularExpressions.Regex')
# The operators that take a pattern on their right: every case variant of match, replace and split.
$script:RegexOperators = @([TokenKind]::Imatch, [TokenKind]::Cmatch, [TokenKind]::Inotmatch, [TokenKind]::Cnotmatch,
    [TokenKind]::Ireplace, [TokenKind]::Creplace, [TokenKind]::Isplit, [TokenKind]::Csplit)
$script:Compares = @([TokenKind]::Ieq, [TokenKind]::Ceq, [TokenKind]::Ine, [TokenKind]::Cne, [TokenKind]::Ilike,
    [TokenKind]::Clike, [TokenKind]::Inotlike, [TokenKind]::Cnotlike, [TokenKind]::Igt, [TokenKind]::Ilt,
    [TokenKind]::Ige, [TokenKind]::Ile, [TokenKind]::Icontains, [TokenKind]::Iin, [TokenKind]::Inotin, [TokenKind]::Inotcontains)

function Read-Options([object[]]$Given) {
    # NOT A PARAM BLOCK: the exe hands every deep half the same `--name value` line, and a script parameter is `-Name`.
    $options = @{ 'list-file' = ''; root = '.'; state = ''; rows = ''; hashes = ''; reset = $false }
    for ($i = 0; $i -lt $Given.Count; $i++) {
        $name = ([string]$Given[$i]).TrimStart('-')
        if ($name -eq 'reset') { $options.reset = $true; continue }
        if (-not $options.ContainsKey($name) -or $i + 1 -ge $Given.Count) { throw "unknown or incomplete option: $($Given[$i])" }
        $i++
        $options[$name] = [string]$Given[$i]
    }
    return $options
}

function Add-Row([string]$Table, [string]$Prefix, $Row) {
    # IDS CONTINUE from the counters the store handed over: an id given out twice is a join that pulls an unrelated row.
    $n = 1
    if ($script:Counters.ContainsKey($Prefix)) { $n = [long]$script:Counters[$Prefix] + 1 }
    $script:Counters[$Prefix] = $n
    $Row['id'] = $Prefix + ':' + $n
    if (-not $script:Tables.Contains($Table)) { $script:Tables[$Table] = [System.Collections.Generic.List[object]]::new() }
    $script:Tables[$Table].Add($Row)
    return $Row['id']
}

function Get-Clipped([string]$Text) {
    if ($Text.Length -le 400) { return $Text }
    return $Text.Substring(0, 400) + ' ...'
}

function Get-LiteralUse([Ast]$Node) {
    # WHAT A LITERAL IS DOING THERE - the half of a magic-value question the value alone cannot answer: an
    # argument of a command or a method (and which), a side of a comparison, or anything else.
    $parent = $Node.Parent
    if ($parent -is [CommandParameterAst]) { $parent = $parent.Parent }
    if ($parent -is [CommandAst]) { return 'argument', ([string]$parent.GetCommandName()) }
    if ($parent -is [InvokeMemberExpressionAst]) { return 'argument', ([string]$parent.Member.Extent.Text) }
    if ($parent -is [BinaryExpressionAst] -and $script:Compares -contains $parent.Operator) { return 'compare', '' }
    return 'other', ''
}

function Get-Pattern($Node) {
    # A pattern is known only from a literal: '' and '' for anything built at run time.
    if ($Node -is [ArrayLiteralAst] -and $Node.Elements.Count -gt 0) { $Node = $Node.Elements[0] }
    if ($Node -is [StringConstantExpressionAst]) { return $Node.Value, 'literal' }
    return '', ''
}

function Get-FunctionRow($Node, $Where, [string]$Kind, $Parameters, $Body, $Prepared) {
    $statements = 0
    $shape = $null
    if ($Body) {
        foreach ($block in @($Body.BeginBlock, $Body.ProcessBlock, $Body.EndBlock)) { if ($block) { $statements += $block.Statements.Count } }
        # THE FILE MAP'S RULE, EXACTLY (`PsMap.ps1`): the end block's statements decide - a class method's too, which the
        # parser carries as a function under its member - and a function declared in a scriptblock literal is that
        # block's own, no duplicate of another block's.
        if ($Body.EndBlock -and $Body.EndBlock.Statements.Count -ge $script:MinBodyStatements -and -not (Get-LiteralScope $Node)) {
            $shape = Get-Fingerprint $Prepared $Body.Extent
        }
        if ($null -eq $Parameters -and $Body.ParamBlock) { $Parameters = $Body.ParamBlock.Parameters }
    }
    $help = ''
    if ($Node -is [FunctionDefinitionAst]) { try { $content = $Node.GetHelpContent(); if ($content -and $content.Synopsis) { $help = $content.Synopsis.Trim() } } catch { $help = '' } }
    $qualname = if ($Where.cls) { $Where.cls + '.' + $Node.Name } else { $Node.Name }
    return [ordered]@{ file = $Where.file; cls = $Where.cls; func = $Where.func; line = $Node.Extent.StartLineNumber
        end_line = $Node.Extent.EndLineNumber; name = $Node.Name; qualname = $qualname; kind = $Kind
        args = [string[]]@(foreach ($p in @($Parameters)) { if ($p) { '$' + $p.Name.VariablePath.UserPath } })
        statements = $statements; body_shape = $(if ($shape) { $shape.Digest } else { '' }); body_size = $(if ($shape) { $shape.Span } else { 0 })
        block = $(if (Get-LiteralScope $Node) { 1 } else { 0 }); doc = $help }
}

function Read-File([string]$Rel, [string]$Abs, [string]$Sha) {
    $parsed = Read-Ast $Abs
    $ast = $parsed.Ast
    $tokens = $parsed.Tokens
    $errors = $parsed.Errors
    $prepared = Get-TokenKeys $tokens
    $text = $ast.Extent.Text
    $lines = $text.Split("`n").Count
    if ($text.EndsWith("`n")) { $lines-- }
    $base = [System.IO.Path]::GetFileName($Rel)
    $dot = $base.IndexOf('.')
    $fileRow = [ordered]@{ path = $Rel; module = $(if ($dot -gt 0) { $base.Substring(0, $dot) } else { $base }); sha = $Sha
        lines = $lines; entry = 1; doc = (Get-Summary $tokens); errors = @($errors).Count }
    $file = Add-Row 'files' 'f' $fileRow
    if ($errors -and $errors.Count -gt 0) {
        # NOT a skip, and no rows from the rest: the parser error-recovers into a PARTIAL tree, and rows read out of it
        # would look exactly like the whole file's. The count rides on the `files` row.
        Write-Record 'MAP-ERROR' $Rel $errors[0].Extent.StartLineNumber ('does not parse under this host: ' + $errors[0].Message)
        return
    }
    # WHERE EACH NODE SITS, in one pass: a node's scope is its parent's unless it opens one, and FindAll hands a parent
    # before its children.
    $top = @{ file = $file; cls = ''; func = '' }
    $scopes = [System.Collections.Generic.Dictionary[Ast, object]]::new()
    foreach ($node in $ast.FindAll($script:EveryNode, $true)) {
        $where = $null
        if (-not ($node.Parent -and $scopes.TryGetValue($node.Parent, [ref]$where))) { $where = $top }
        $scopes[$node] = $where
        $line = $node.Extent.StartLineNumber
        if ($node -is [FunctionDefinitionAst] -and $node.Parent -is [FunctionMemberAst]) {
            # THE METHOD'S OWN BODY, as the parser holds it: the method is its member's row.
            $scopes[$node] = $where
        } elseif ($node -is [FunctionDefinitionAst]) {
            $fileRow.entry = 0
            $kind = if ($node.IsFilter) { 'filter' } elseif ($node.IsWorkflow) { 'workflow' } else { 'function' }
            [void](Add-Row 'functions' 'fn' (Get-FunctionRow $node $where $kind $node.Parameters $node.Body $prepared))
            $scopes[$node] = @{ file = $file; cls = $where.cls; func = $node.Name }
        } elseif ($node -is [TypeDefinitionAst]) {
            $kind = if ($node.IsEnum) { 'enum' } elseif ($node.IsInterface) { 'interface' } else { 'class' }
            [void](Add-Row 'classes' 'c' ([ordered]@{ file = $file; cls = $where.cls; func = $where.func; line = $line
                end_line = $node.Extent.EndLineNumber; name = $node.Name; qualname = $node.Name; kind = $kind
                bases = [string[]]@(foreach ($b in $node.BaseTypes) { $b.TypeName.FullName }); members = $node.Members.Count }))
            $scopes[$node] = @{ file = $file; cls = $node.Name; func = '' }
        } elseif ($node -is [FunctionMemberAst]) {
            $kind = if ($node.IsConstructor) { 'constructor' } else { 'method' }
            [void](Add-Row 'functions' 'fn' (Get-FunctionRow $node $where $kind $node.Parameters $node.Body $prepared))
            $scopes[$node] = @{ file = $file; cls = $where.cls; func = $node.Name }
        } elseif ($node -is [CommandAst]) {
            $name = $node.GetCommandName()
            $first = $node.CommandElements[0].Extent.Text
            if ($node.InvocationOperator -eq [TokenKind]::Dot) {
                [void](Add-Row 'imports' 'i' ([ordered]@{ file = $file; cls = $where.cls; func = $where.func; line = $line; module = $first; kind = 'dot' }))
                continue
            }
            if ($name -eq 'Import-Module' -and $node.CommandElements.Count -gt 1) {
                [void](Add-Row 'imports' 'i' ([ordered]@{ file = $file; cls = $where.cls; func = $where.func; line = $line
                    module = $node.CommandElements[1].Extent.Text; kind = 'module' }))
            }
            [void](Add-Row 'calls' 'call' ([ordered]@{ file = $file; cls = $where.cls; func = $where.func; line = $line
                callee = $(if ($name) { $name } else { $first }); kind = $(if ($node.InvocationOperator -eq [TokenKind]::Ampersand) { 'invoke' } else { 'command' })
                args = $node.CommandElements.Count - 1; source = (Get-Clipped $node.Extent.Text) }))
        } elseif ($node -is [UsingStatementAst] -and $node.UsingStatementKind -eq [UsingStatementKind]::Module -and $node.Name) {
            [void](Add-Row 'imports' 'i' ([ordered]@{ file = $file; cls = $where.cls; func = $where.func; line = $line; module = $node.Name.Extent.Text; kind = 'using' }))
        } elseif ($node -is [IfStatementAst]) {
            $n = 0
            foreach ($clause in $node.Clauses) {
                [void](Add-Row 'branches' 'br' ([ordered]@{ file = $file; cls = $where.cls; func = $where.func; line = $clause.Item1.Extent.StartLineNumber
                    end_line = $clause.Item2.Extent.EndLineNumber; kind = $(if ($n -eq 0) { 'if' } else { 'elseif' }); test = $clause.Item1.Extent.Text }))
                $n++
            }
        } elseif ($node -is [LoopStatementAst] -or $node -is [SwitchStatementAst]) {
            $kind = $node.GetType().Name.Replace('StatementAst', '').ToLowerInvariant()
            $test = if ($node.Condition) { $node.Condition.Extent.Text } else { '' }
            [void](Add-Row 'branches' 'br' ([ordered]@{ file = $file; cls = $where.cls; func = $where.func; line = $line
                end_line = $node.Extent.EndLineNumber; kind = $kind; test = $test }))
            if ($node -is [SwitchStatementAst] -and ($node.Flags -band [SwitchFlags]::Regex)) {
                foreach ($clause in $node.Clauses) {
                    $pattern, $patternKind = Get-Pattern $clause.Item1
                    [void](Add-Row 'regexes' 'rx' ([ordered]@{ file = $file; cls = $where.cls; func = $where.func; line = $clause.Item1.Extent.StartLineNumber
                        kind = 'switch'; api = 'switch -Regex'; pattern = $pattern; pattern_kind = $patternKind; source = $clause.Item1.Extent.Text }))
                }
            }
        } elseif ($node -is [TryStatementAst]) {
            [void](Add-Row 'branches' 'br' ([ordered]@{ file = $file; cls = $where.cls; func = $where.func; line = $line
                end_line = $node.Extent.EndLineNumber; kind = 'try'; test = '' }))
            foreach ($catch in $node.CatchClauses) {
                [void](Add-Row 'handlers' 'h' ([ordered]@{ file = $file; cls = $where.cls; func = $where.func; line = $catch.Extent.StartLineNumber
                    end_line = $catch.Extent.EndLineNumber; try_line = $line; kind = 'catch'; types = [string[]]@(foreach ($t in $catch.CatchTypes) { $t.TypeName.FullName })
                    bare = $(if ($catch.IsCatchAll) { 1 } else { 0 }); statements = $catch.Body.Statements.Count }))
            }
        } elseif ($node -is [TrapStatementAst]) {
            [void](Add-Row 'handlers' 'h' ([ordered]@{ file = $file; cls = $where.cls; func = $where.func; line = $line
                end_line = $node.Extent.EndLineNumber; try_line = 0; kind = 'trap'; types = [string[]]@(if ($node.TrapType) { $node.TrapType.TypeName.FullName })
                bare = $(if ($node.TrapType) { 0 } else { 1 }); statements = $node.Body.Statements.Count }))
        } elseif ($node -is [ThrowStatementAst]) {
            [void](Add-Row 'raises' 'rs' ([ordered]@{ file = $file; cls = $where.cls; func = $where.func; line = $line
                source = $(if ($node.Pipeline) { Get-Clipped $node.Pipeline.Extent.Text } else { '' }); rethrow = $(if ($node.IsRethrow) { 1 } else { 0 }) }))
        } elseif ($node -is [AssignmentStatementAst]) {
            [void](Add-Row 'assignments' 'a' ([ordered]@{ file = $file; cls = $where.cls; func = $where.func; line = $line
                target = $node.Left.Extent.Text; operator = $node.ErrorPosition.Text; source = (Get-Clipped $node.Right.Extent.Text) }))
        }

        if ($node -isnot [ExpressionAst]) { continue }
        if ($node -is [StringConstantExpressionAst]) {
            $parent = $node.Parent
            $isName = ($parent -is [CommandAst] -and $parent.CommandElements[0] -eq $node) -or ($parent -is [MemberExpressionAst] -and $parent.Member -eq $node)
            if (-not $isName -and $node.Value.Trim().Length -gt 0) {
                $use, $callee = Get-LiteralUse $node
                [void](Add-Row 'string_literals' 's' ([ordered]@{ file = $file; cls = $where.cls; func = $where.func; line = $line
                    value = $node.Value; length = $node.Value.Length; kind = [string]$node.StringConstantType; use = $use; callee = $callee }))
            }
        } elseif ($node -is [ExpandableStringExpressionAst]) {
            $use, $callee = Get-LiteralUse $node
            [void](Add-Row 'string_literals' 's' ([ordered]@{ file = $file; cls = $where.cls; func = $where.func; line = $line
                value = $node.Value; length = $node.Value.Length; kind = 'expandable'; use = $use; callee = $callee }))
        } elseif ($node -is [ConstantExpressionAst] -and $null -ne $node.Value -and $node.Value.GetType().IsPrimitive -and $node.Value -isnot [bool] -and $node.Value -isnot [char]) {
            $use, $callee = Get-LiteralUse $node
            [void](Add-Row 'number_literals' 'n' ([ordered]@{ file = $file; cls = $where.cls; func = $where.func; line = $line
                value = $node.Extent.Text; number = [double]$node.Value; use = $use; callee = $callee }))
        } elseif ($node -is [InvokeMemberExpressionAst]) {
            $receiver = $node.Expression.Extent.Text
            $operator = if ($node.Static) { '::' } else { '.' }
            $member = $node.Member.Extent.Text
            [void](Add-Row 'calls' 'call' ([ordered]@{ file = $file; cls = $where.cls; func = $where.func; line = $line
                callee = $receiver + $operator + $member; kind = $(if ($node.Static) { 'static' } else { 'method' })
                args = @($node.Arguments).Count; source = (Get-Clipped $node.Extent.Text) }))
            if ($node.Static -and $node.Expression -is [TypeExpressionAst] -and $script:RegexTypes -contains $node.Expression.TypeName.FullName) {
                $pattern, $patternKind = Get-Pattern $(if (@($node.Arguments).Count -gt 1) { $node.Arguments[1] } else { $null })
                [void](Add-Row 'regexes' 'rx' ([ordered]@{ file = $file; cls = $where.cls; func = $where.func; line = $line
                    kind = 'call'; api = 'regex::' + $member; pattern = $pattern; pattern_kind = $patternKind; source = (Get-Clipped $node.Extent.Text) }))
            }
        } elseif ($node -is [ConvertExpressionAst] -and $script:RegexTypes -contains $node.Type.TypeName.FullName) {
            $pattern, $patternKind = Get-Pattern $node.Child
            [void](Add-Row 'regexes' 'rx' ([ordered]@{ file = $file; cls = $where.cls; func = $where.func; line = $line
                kind = 'cast'; api = 'regex cast'; pattern = $pattern; pattern_kind = $patternKind; source = (Get-Clipped $node.Extent.Text) }))
        } elseif ($node -is [BinaryExpressionAst] -and $script:RegexOperators -contains $node.Operator) {
            $pattern, $patternKind = Get-Pattern $node.Right
            [void](Add-Row 'regexes' 'rx' ([ordered]@{ file = $file; cls = $where.cls; func = $where.func; line = $line
                kind = 'operator'; api = $node.ErrorPosition.Text.ToLowerInvariant(); pattern = $pattern; pattern_kind = $patternKind; source = (Get-Clipped $node.Extent.Text) }))
        }

        # THE FILE MAP'S RULE FOR AN EXPRESSION ROW, EXACTLY (`PsMap.ps1`): characters first, then tokens.
        $extent = $node.Extent
        if (($extent.EndOffset - $extent.StartOffset) -lt $script:MinExpressionChars) { continue }
        # TOO FEW TOKENS IS ASKED HERE, before the call, as the file map does: most long expressions are too small.
        $first, $last = Get-TokenRange $prepared $extent
        if (($prepared.Counted[$last] - $prepared.Counted[$first]) -lt $script:MinExpressionTokens) { continue }
        $shape = Get-Fingerprint $prepared $extent
        if ($shape.Tokens -lt $script:MinExpressionTokens) { continue }
        $reads = [System.Collections.Generic.List[string]]::new()
        foreach ($v in $node.FindAll($script:Variables, $true)) {
            $read = $v.VariablePath.UserPath
            if (-not $reads.Contains($read)) { $reads.Add($read) }
        }
        [void](Add-Row 'expressions' 'x' ([ordered]@{ file = $file; cls = $where.cls; func = $where.func; line = $line
            role = $node.GetType().Name.Replace('Ast', ''); source = $extent.Text; size = $shape.Tokens; span = $shape.Span
            shape = $shape.Digest; reads = $reads.ToArray() }))
    }
}

$options = Read-Options $args
$state = [System.IO.File]::ReadAllText($options.state) | ConvertFrom-Json
$handed = @{}
if ($options.hashes) {
    $given = [System.IO.File]::ReadAllText($options.hashes) | ConvertFrom-Json
    foreach ($p in $given.PSObject.Properties) { $handed[$p.Name] = [string]$p.Value }
}
# HASH FIRST, DECIDE SECOND: a file the tree map already hashed is not READ unless it is parsed - its sha is that hash
# under this half's version.
$list = [System.Collections.Generic.List[object]]::new()
$shas = [ordered]@{}
foreach ($raw in [System.IO.File]::ReadAllLines($options.'list-file')) {
    $parts = $raw.Split("`t")
    if ($parts.Count -lt 2 -or $parts[0].Trim().Length -eq 0) { continue }
    $rel = $parts[0].Trim()
    $abs = $parts[1].Trim()
    $content = ''
    if ($handed.ContainsKey($rel)) { $content = 'tree ' + $handed[$rel] }
    else {
        try { $content = [System.IO.File]::ReadAllText($abs) } catch { Write-Record 'MAP-ERROR' $rel 1 ('cannot be read - ' + $_.Exception.Message); continue }
    }
    $list.Add(@($rel, $abs))
    $shas[$rel] = Get-Hex ($script:RowsVersion + "`n" + $content) 16
}
$full = [bool]$state.rebuild
$before = @{}
if (-not ($full -or $options.reset) -and $state.shas) { foreach ($p in $state.shas.PSObject.Properties) { $before[$p.Name] = [string]$p.Value } }
# IDS CONTINUE unless the database is being rebuilt: a reset drops only THIS half's rows, and the other halves share
# several prefixes (`f`, `fn`, `call`) - restarting would hand out an id they already hold.
if (-not $full -and $state.counters) { foreach ($p in $state.counters.PSObject.Properties) { $script:Counters[$p.Name] = [long]$p.Value } }

$read = [System.Collections.Generic.List[object]]::new()
foreach ($row in $list) {
    $rel = $row[0]
    if ($before.ContainsKey($rel) -and $before[$rel] -eq $shas[$rel]) { continue }
    try {
        Read-File $rel $row[1] $shas[$rel]
        $read.Add($row)
    } catch {
        Write-Record 'MAP-ERROR' $rel 1 ('could not be read - ' + $_.Exception.Message)
        $shas.Remove($rel)
    }
    Write-Records
}

# THE TEXT IS NOT SENT: the store reads each file itself off `read`.
$payload = [ordered]@{ all = $true; first = $true; final = $true; reset = [bool]$options.reset; shas = $shas
    read = $read.ToArray(); counters = $script:Counters; tables = $script:Tables; lang = 'powershell' }
# SERIALIZED IN .NET, ONE CALL: a writer in script made a call per cell and was most of a cold run. 5.1 has no
# System.Text.Json; its own serializer takes the same dictionaries and lists. Every array cell is a typed .NET array, never
# a pipeline's wrapped objects, so both write plain values.
if ($PSVersionTable.PSVersion.Major -ge 6) {
    $text = [System.Text.Json.JsonSerializer]::Serialize($payload, [object], [System.Text.Json.JsonSerializerOptions]::new())
} else {
    Add-Type -AssemblyName System.Web.Extensions
    $serializer = [System.Web.Script.Serialization.JavaScriptSerializer]::new()
    $serializer.MaxJsonLength = [int]::MaxValue
    $text = $serializer.Serialize($payload)
}
[System.IO.File]::WriteAllText($options.rows, $text, [System.Text.UTF8Encoding]::new($false))
Write-Record 'MAP-READ' $read.Count $shas.Count
Write-Record 'MAP-DONE' $shas.Count
Write-Records
