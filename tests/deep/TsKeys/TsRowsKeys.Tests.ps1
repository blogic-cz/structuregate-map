<#
    A key the code names through something OTHER than a literal at the use: a member of a const object
    (`KEYS.hint`), a `+` chain, and a locale value that is an ARRAY read whole by an i18n call. Each read as
    nothing before, so a field bound to the key reached no template, and a defined list read as missing.
#>

# In a folder of its own only because `tests/deep/` is at its file limit; the helpers stay beside the suites
# they were written for.
. (Join-Path $PSScriptRoot '..\TsRows.Helpers.ps1')

if ($script:TsRowsModules -and $script:TsRowsPython) {

# One component naming its keys through const objects - its own file's and another file's - plus the
# negative control: a const LIST of rule objects, returned by a bound getter, holds a key without being it.
function New-TsRowsKeysConstTree {
    New-TsRowsWorkspace @{
        'apps/shop/src/keys.ts' = "export const SHARED = { hint: 'set.sharedhint' };`n"
        'apps/shop/src/settings.component.html' = "<b [title]=`"hint`">h</b><i [title]=`"sharedHint`">s</i>" +
            "<u [title]=`"tip`">t</u><s [title]=`"rules`">r</s>`n"
        'apps/shop/src/settings.component.ts' = "import { Component } from '@angular/core';`n" +
            "import { SHARED } from './keys';`n" +
            "export enum Kind { A = 1, B = 2 }`n" +
            "const KEYS = { hint: 'set.hint', guarded: 'set.guarded' };`n" +
            "const RULES = [{ Field: 'x', Message: 'set.v' }];`n" +
            "@Component({ selector: 'app-settings', templateUrl: './settings.component.html', standalone: true })`n" +
            "export class SettingsComponent {`n" +
            "  kind: Kind = Kind.A;`n" +
            "  hint = KEYS.hint;`n" +
            "  sharedHint = SHARED.hint;`n" +
            "  tip = '';`n" +
            "  pick(): void {`n" +
            "    if (this.kind === Kind.B) {`n      this.tip = KEYS.guarded;`n    }`n" +
            "  }`n" +
            "  get rules(): { Field: string; Message: string }[] {`n    return RULES;`n  }`n" +
            "}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop","cart":{"empty":"Empty"}},' +
            '"set":{"hint":"a","sharedhint":"b","guarded":"c","v":"d"}}'
    }
}

Test-Case 'tsrows keys: a const object member bound through a field is a field binding' {
    $made = New-TsRowsDb (New-TsRowsKeysConstTree)
    $r = Invoke-TsRowsQ $made.Db "SELECT key || ' | ' || route AS reach FROM key_reach WHERE key = 'set.hint'"
    Assert-Line $r 'set.hint | field_binding'
}

# THE KEY SITS IN A FILE THAT DECLARES NO COMPONENT, so its literal reaches none: only the field it is
# read into, and the node binding that field, name the component it renders in.
Test-Case 'tsrows keys: a const object from another file reaches the component that binds its member' {
    $made = New-TsRowsDb (New-TsRowsKeysConstTree)
    $r = Invoke-TsRowsQ $made.Db ("SELECT 'hits=' || count(*) AS reach FROM key_reach k, json_each(k.components) j " +
        "JOIN components c ON c.id = j.value WHERE k.key = 'set.sharedhint' AND c.name = 'SettingsComponent'")
    Assert-Line $r 'hits=1'
}

Test-Case 'tsrows keys: a const object member assigned under an if carries that branch' {
    $made = New-TsRowsDb (New-TsRowsKeysConstTree)
    $r = Invoke-TsRowsQ $made.Db ("SELECT key || ' | ' || json_array_length(always_branches) || ' | ' || " +
        "json_extract(always_values, '$[0].values') AS reach FROM key_reach WHERE key = 'set.guarded'")
    Assert-Line $r 'set.guarded | 1 | ["B"]'
}

# A LIST OF OBJECTS IS NOT A KEY. `RULES` holds `set.v` and the getter bound in the template returns it, but
# no value is the key - reaching into it would claim every string of every rule object.
Test-Case 'tsrows keys: a key inside a const list of objects stays a bare literal' {
    $made = New-TsRowsDb (New-TsRowsKeysConstTree)
    $r = Invoke-TsRowsQ $made.Db "SELECT key || ' | ' || route AS reach FROM key_reach WHERE key = 'set.v'"
    Assert-Line $r 'set.v | literal'
    Assert-NoLine $r 'field_binding'
}

# A locale file whose values include LISTS - of strings and of objects - read by a service method that returns
# the whole list, and a `+` chain whose tail is a parameter.
function New-TsRowsKeysLocaleTree {
    New-TsRowsWorkspace @{
        'apps/shop/src/regions.component.html' = "<b>{{ south }}</b><i>{{ plain }}</i>`n"
        'apps/shop/src/regions.component.ts' = "import { Component, Injectable } from '@angular/core';`n" +
            "@Injectable({ providedIn: 'root' })`n" +
            "export class Words {`n" +
            "  instant(key: string): string {`n    return key;`n  }`n" +
            "  dict(key: string): string[] {`n    return [key];`n  }`n" +
            "}`n" +
            "@Component({ selector: 'app-regions', templateUrl: './regions.component.html', standalone: true })`n" +
            "export class RegionsComponent {`n" +
            "  constructor(private words: Words) {}`n" +
            "  get south(): string[] {`n    return this.words.dict('regions.South');`n  }`n" +
            "  get plain(): string {`n    return this.words.instant('regions.' + 'Plain');`n  }`n" +
            "  label(kind: string): string {`n    return this.words.instant('labels.' + kind);`n  }`n" +
            "}`n"
        'apps/shop/src/assets/locales/en.json' = '{"shop":{"title":"Shop","cart":{"empty":"Empty"}},' +
            '"regions":{"North":["Alpha","Beta"],"South":["Gamma"],"West":[{"name":"Delta"}],' +
            '"Plain":"p"},"labels":{"laptop":"a"}}'
        'structuregate.ts.json' = '{"locales":["apps/shop/src/assets/locales"],"i18nCarriers":["money"],' +
            '"i18nCalls":["instant","dict"],"gateInputs":["disabled"],' +
            '"featureChecks":["FlagService.isOn"],"featureEnum":"FlagCodes"}'
    }
}

Test-Case 'tsrows keys: a locale array becomes one row per element, named by its index' {
    $made = New-TsRowsDb (New-TsRowsKeysLocaleTree)
    $r = Invoke-TsRowsQ $made.Db "SELECT key || ' | ' || text AS tr FROM translations WHERE key LIKE 'regions.%'"
    Assert-Line $r 'regions.North[0] | Alpha'
    Assert-Line $r 'regions.North[1] | Beta'
    Assert-Line $r 'regions.West[0].name | Delta'
}

# THE WHOLE LIST IS WHAT THE CALL READS, and the file defines it: no gap, and the locale that defines it.
Test-Case 'tsrows keys: an i18n call reading a whole locale array is a defined key' {
    $made = New-TsRowsDb (New-TsRowsKeysLocaleTree)
    $r = Invoke-TsRowsQ $made.Db ("SELECT key || ' | ' || coalesce(defined_in, 'none') AS ix FROM i18n_index " +
        "WHERE key IN ('regions.South', 'regions.Plain')")
    Assert-Line $r 'regions.South | ["en"]'
    Assert-Line $r 'regions.Plain | ["en"]'
    # NO KEY IN THIS TREE IS UNDEFINED, so the column is not there at all - a column no row sets is never
    # created. Asked of the schema, because `undefined_key` itself cannot be selected when it is absent.
    $gap = Invoke-TsRowsQ $made.Db "SELECT 'undefined_columns=' || count(*) AS c FROM pragma_table_info('i18n_index') WHERE name = 'undefined_key'"
    Assert-Line $gap 'undefined_columns=0'
}

Test-Case 'tsrows keys: a locale array element is referenced exactly when its array is' {
    $made = New-TsRowsDb (New-TsRowsKeysLocaleTree)
    $r = Invoke-TsRowsQ $made.Db ("SELECT key || ' | ' || referenced AS tr FROM translations " +
        "WHERE key IN ('regions.South[0]', 'regions.North[0]')")
    Assert-Line $r 'regions.South[0] | 1'
    Assert-Line $r 'regions.North[0] | 0'
}

# A `+` CHAIN WITH A HOLE is every key its pieces spell, as a template literal is: `'labels.' + kind` is no
# static key, so no i18n ref names `labels.laptop` - the chain is the only evidence it is used here.
Test-Case 'tsrows keys: a key built by a + chain reaches the component it is built in' {
    $made = New-TsRowsDb (New-TsRowsKeysLocaleTree)
    $r = Invoke-TsRowsQ $made.Db ("SELECT key || ' | ' || route || ' | ' || json_array_length(components) AS reach " +
        "FROM key_reach WHERE key = 'labels.laptop'")
    Assert-Line $r 'labels.laptop | literal | 1'
}

}
