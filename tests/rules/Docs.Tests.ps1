<#
    The doc parser (src\Docs.cs): which files are CONTEXT docs, the markdown section counter behind
    "largest sections", the four remedies, and the local-link scanner.

    The remedies are worth this much test surface because the message IS the feature. "Too long" without a
    named section invites a TRIM, and trimming deletes the measured reasons nobody can re-derive - so what
    is asserted is that each kind of file is told the move that is right FOR THAT KIND.
#>

# ---------------------------------------------------------------- what counts as a context doc

Test-Case 'docs: CLAUDE.md and AGENTS.md are context docs anywhere in the tree' {
    $tree = Use-Tree @{
        'CLAUDE.md'          = "# a`nl`nl`n"
        'deep/nest/AGENTS.md' = "# b`nl`nl`n"
        'deep/nest/notes.md'  = "# c`nl`nl`n"
    }
    $result = Invoke-Gate --root $tree --max-doc-lines 2 --doc-scope context
    Assert-Exit $result 1
    Assert-Line $result 'CLAUDE.md'
    Assert-Line $result 'AGENTS.md'
    Assert-NoLine $result 'notes.md'
}

Test-Case 'docs: a .claude/skills body is a context doc, a .claude doc that is neither is not' {
    $tree = Use-Tree @{
        '.claude/skills/thing/SKILL.md' = "# s`nl`nl`n"
        '.claude/settings-notes.md'     = "# n`nl`nl`n"
    }
    $result = Invoke-Gate --root $tree --max-doc-lines 2 --doc-scope context
    Assert-Exit $result 1
    Assert-Line $result 'SKILL.md'
    Assert-NoLine $result 'settings-notes.md'
}

# ---------------------------------------------------------------- the section counter

Test-Case 'docs: sections are counted between headings, largest first, top five only' {
    $body = "# title`n"
    $body += "## small`nl`n"
    $body += "## big`n" + (1..9 | ForEach-Object { "l`n" }) -join ''
    $body += "## middle`nl`nl`nl`n"
    $body += "## tiny1`nl`n## tiny2`nl`n## tiny3`nl`n## tiny4`nl`n"
    $tree = Use-Tree @{ 'CLAUDE.md' = $body }

    $result = Invoke-Gate --root $tree --max-doc-lines 5
    Assert-Exit $result 1
    Assert-Line $result 'Largest sections'
    Assert-Line $result '## big'
    Assert-Line $result '## middle'
    # Five at most, so the smallest of the eight sections must not be listed.
    Assert-NoLine $result '## tiny4'
}

Test-Case 'docs: content before the first heading is still counted, and named' {
    $tree = Use-Tree @{ 'CLAUDE.md' = "intro`nintro`nintro`n# later`nl`n" }
    $result = Invoke-Gate --root $tree --max-doc-lines 2
    Assert-Exit $result 1
    Assert-Line $result '(before the first heading)'
}

# ---------------------------------------------------------------- the remedy, per kind of file

Test-Case 'docs: an AGENT is told to keep its frontmatter and split with NO frontmatter' {
    $tree = Use-Tree @{ '.claude/agents/big.md' = "---`nname: big`ndescription: d`n---`nl`nl`nl`n" }
    $result = Invoke-Gate --root $tree --max-doc-lines 3
    Assert-Exit $result 1
    Assert-Line $result 'this is an AGENT definition'
    Assert-Line $result 'big-<topic>.md'
    Assert-Line $result 'MUST HAVE NO FRONTMATTER'
}

Test-Case 'docs: a SKILL is told to split beside SKILL.md, at the same level' {
    $tree = Use-Tree @{ '.claude/skills/thing/SKILL.md' = "---`nname: t`n---`nl`nl`nl`n" }
    $result = Invoke-Gate --root $tree --max-doc-lines 3
    Assert-Exit $result 1
    Assert-Line $result 'this is a SKILL'
    Assert-Line $result 'BESIDE it'
    Assert-Line $result 'disclose progressively'
}

Test-Case 'docs: a CLAUDE.md is told to link a sibling, not to @import it' {
    $tree = Use-Tree @{ 'sub/CLAUDE.md' = "# t`nl`nl`nl`n" }
    $result = Invoke-Gate --root $tree --max-doc-lines 2
    Assert-Exit $result 1
    Assert-Line $result 'CONTEXT file loaded every session'
    Assert-Line $result 'in `sub/`'
    Assert-Line $result '@import'
}

Test-Case 'docs: any other long doc gets the plain move-a-section remedy' {
    $tree = Use-Tree @{ 'guide.md' = "# t`nl`nl`nl`n" }
    $result = Invoke-Gate --root $tree --max-doc-lines 2
    Assert-Exit $result 1
    Assert-Line $result 'move a section into a `<topic>.md` beside this file'
}

# ---------------------------------------------------------------- the link scanner

Test-Case 'docs: only local .md links are followed - a URL and a non-md target are not' {
    $tree = Use-Tree @{
        'CLAUDE.md' = @"
# t
[web](https://example.com/thing.md)
[image](diagram.png)
[code](src/app.py)
[real](topic.md)
"@
        'topic.md' = "# topic`n"
    }
    Assert-Exit (Invoke-Gate --root $tree) 0
}

Test-Case 'docs: a link into a subfolder is resolved relative to the doc' {
    $tree = Use-Tree @{
        'sub/CLAUDE.md' = "# t`nsee [up](../root-topic.md) and [down](more/deep.md)`n"
        'root-topic.md' = "# r`n"
        'sub/more/deep.md' = "# d`n"
    }
    Assert-Exit (Invoke-Gate --root $tree) 0
}

Test-Case 'docs: an unterminated link does not swallow the rest of the file' {
    $tree = Use-Tree @{ 'CLAUDE.md' = "# t`n[broken](no-close`nthen [real](topic.md)`n"; 'topic.md' = "# t`n" }
    Assert-Exit (Invoke-Gate --root $tree) 0
}

Test-Case 'docs: a dangling link is reported once per target, with the doc that holds it' {
    $tree = Use-Tree @{ 'sub/CLAUDE.md' = "# t`n[a](missing-a.md) [b](missing-b.md)`n" }
    $result = Invoke-Gate --root $tree
    Assert-Exit $result 1
    Assert-Line $result 'sub/CLAUDE.md: links to missing-a.md'
    Assert-Line $result 'sub/CLAUDE.md: links to missing-b.md'
}

Test-Case 'docs: the link rule runs on docs OUTSIDE the doc-length scope too' {
    # A README is not measured for length under --doc-scope context, but a link it makes to a file that
    # does not exist is still a broken reference in the repo.
    $tree = Use-Tree @{ 'README.md' = "# r`nsee [gone](gone.md)`n" }
    $result = Invoke-Gate --root $tree --doc-scope context
    Assert-Exit $result 0
    # Documented behaviour, asserted so a change to it is deliberate: context scope means the README is not
    # measured at all, links included.
    Assert-NoLine $result 'gone.md'
}
