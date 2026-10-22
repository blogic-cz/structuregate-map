<#
    A MEMBER A TYPE ALIAS BORROWS FROM A GENERIC IS TYPED AS THE ALIAS INSTANTIATES IT (#96, `TsDecls/TsTypeRows.mjs::typeMemberRowsFromType`):
    `type PayloadBox = Box<Payload>` gives its computed member `Payload` the type `Payload`, never `Box`'s type parameter, so a
    template read past it resolves. Its helpers are `TsRows.Helpers.ps1`.
#>

. (Join-Path $PSScriptRoot '../../TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

$script:TsComputedFiles = @{
    'apps/shop/src/box.component.ts' = "import { Component } from '@angular/core';`n" +
        "export interface Flags { On: boolean; }`n" +
        "export interface Payload { Flags: Flags; }`n" +
        "export class Box<TPayload> { Payload!: TPayload; Note!: string; }`n" +
        "export type PayloadBox = Box<Payload>;`n" +
        "class Holder { get<T>(): T { return {} as T; } }`n" +
        "@Component({ selector: 'app-box', template: '<p>{{ vm?.Payload.Flags.On }}</p>' })`n" +
        "export class BoxComponent {`n" +
        "  holder = new Holder();`n" +
        "  get vm() { return this.holder.get<PayloadBox>(); }`n" +
        "}`n"
    'apps/shop/src/clash.models.ts' = @'
export interface Idd { Code: string; }
export class Base<T> { Input: { Id?: number } = {}; Extra?: T; }
export type AlphaVm<T> = Base<T> & {
  Input: {
    IDD?: Idd;
  };
};
export interface Opt { Kind: string; Text: string; }
export type View = Omit<Opt & { Text: string; Extra: string }, 'Kind'>;
export type Joined = Opt & { Text: string };
export type Either = { Tag: string; Left: number } | { Tag: string; Right: number };
'@
}

Test-Case 'tsrows: a member an alias borrows from a generic is typed as the alias instantiates it, and reads resolve past it (#96)' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsComputedFiles)
    Assert-Exit $made.Result 0
    $members = Invoke-TsRowsQ $made.Db ("SELECT 'tm=' || m.name || ':' || coalesce(m.type, '-') AS r FROM type_members m " +
        "JOIN type_aliases a ON a.id = m.owner WHERE a.name = 'PayloadBox' AND m.via = 'computed' ORDER BY m.name")
    Assert-Line $members 'tm=Payload:Payload'
    # A member nothing instantiates keeps its annotation's text.
    Assert-Line $members 'tm=Note:string'
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'targets=' || group_concat(n.value, ',') AS r FROM (SELECT j.owner, j.n FROM expressions x " +
        "JOIN expressions__ast j ON j.owner = x.id AND j.role = 'target' WHERE x.source = '{{ vm?.Payload.Flags.On }}' ORDER BY j.n) t " +
        "JOIN expressions__ast n ON n.owner = t.owner AND n.parent = t.n AND n.role = 'name'")
    Assert-Line $r 'targets=vm,Payload,Flags,On'
}


# A LITERAL'S OWN MEMBER IS A ROW EVEN WHEN ANOTHER OPERAND DECLARES THE NAME (rows 75, `TsDecls/TsTypeRows.mjs::typeMemberRowsFromType`): `Base<T> & { Input: { .. } }` over a
# class with its own `Input` is ONE property to the checker, whose computed row sits at the first declaration - and the literal's line had no row, nor did the
# members below it hang from it. The controls: the computed row at the first declaration, and the member a literal alone declares.
Test-Case 'tsrows: a type literal member another operand also declares has its own row, at its own line' {
    $made = New-TsRowsDb (New-TsRowsWorkspace $script:TsComputedFiles)
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'tm=' || a.name || ':' || m.path || '@' || m.line AS r FROM type_members m JOIN type_aliases a ON a.id = m.owner " +
        "JOIN files f ON f.id = a.file WHERE f.path LIKE '%clash.models.ts' ORDER BY a.name, m.line, m.path")
    Assert-Line $r 'tm=AlphaVm:Input@4'
    Assert-Line $r 'tm=Joined:Text@10'
    Assert-Line $r 'tm=View:Text@9'
    Assert-Line $r 'tm=AlphaVm:Input@2'
    Assert-Line $r 'tm=AlphaVm:Input.IDD@5'
    Assert-Line $r 'tm=Joined:Text@8'
    Assert-Line $r 'tm=View:Extra@9'
    # The member below the literal's own row hangs from IT, not from the borrowed one.
    $p = Invoke-TsRowsQ $made.Db ("SELECT 'under=' || p.line AS r FROM type_members m JOIN type_members p ON p.id = m.parent JOIN type_aliases a ON a.id = m.owner " +
        "WHERE a.name = 'AlphaVm' AND m.path = 'Input.IDD'")
    Assert-Line $p 'under=4'
    # Both arms of a union declare `Tag`: two rows.
    $u = Invoke-TsRowsQ $made.Db ("SELECT 'tags=' || count(*) AS r FROM type_members m JOIN type_aliases a ON a.id = m.owner WHERE a.name = 'Either' AND m.path = 'Tag'")
    Assert-Line $u 'tags=2'
    Assert-Line (Invoke-TsRowsQ $made.Db "SELECT 'left=' || count(*) AS r FROM type_members m JOIN type_aliases a ON a.id = m.owner WHERE a.name = 'Either' AND m.path = 'Left'") 'left=1'
}


# (rows 80) A CALL'S `target_id` IS THE DECLARATION THE CHECKER NAMED, found by its PLACE (`TsRefs.mjs::rollupRefIds`, `TsExpr/TsReadRefs.mjs::targetLine`): `Ident.getAge(..)` is the property of an object literal -
# its `shape_members` row - though `function getAge` sits in the same file, and `this.mask.writeValue(..)` is the member of a type literal (`type_members`), not the class's own `writeValue`. Controls: a property
# that only names a function (`getAge: getAge`, `{ getAge }`) still follows the declaration it names.
Test-Case 'tsrows: a call of an object literal or type member has the row at the checker declaration, not a same-named one' {
    $tree = New-TsRowsWorkspace @{
        'apps/shop/src/target.ts' = "export function getAge(d: string): number { return d.length; }`n" +
            "export const normalize = (s: string) => s.trim();`n" +
            "export const Ident = {`n  getAge: (d: string) => d.length + 1,`n  normalize(s: string) { return s; },`n};`n" +
            "export const Same = { getAge: getAge };`nexport const Short = { getAge };`n" +
            "type MaskType = { writeValue: (v: string) => void };`n" +
            "export class Field {`n  mask!: MaskType;`n  writeValue(v: string) { this.mask.writeValue(v); }`n" +
            "  run() { Ident.getAge('a'); Ident.normalize('b'); Same.getAge('c'); Short.getAge('d'); }`n}`n"
    }
    $made = New-TsRowsDb $tree -Shared
    Assert-Exit $made.Result 0
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'tg=' || c.callee || '|' || coalesce((SELECT 'sm@' || line FROM shape_members WHERE id = c.target_id), (SELECT 'tm@' || line FROM type_members WHERE id = c.target_id), " +
        "(SELECT 'fn@' || line FROM functions WHERE id = c.target_id), (SELECT 'm@' || line FROM members WHERE id = c.target_id), 'none') AS r FROM calls c JOIN files f ON f.id = c.file WHERE f.path LIKE '%target.ts'")
    Assert-Line $r 'tg=Ident.getAge|sm@4'
    Assert-Line $r 'tg=Ident.normalize|sm@5'
    Assert-Line $r 'tg=this.mask.writeValue|tm@9'
    Assert-Line $r 'tg=Same.getAge|fn@1'
    Assert-Line $r 'tg=Short.getAge|fn@1'
}
}
