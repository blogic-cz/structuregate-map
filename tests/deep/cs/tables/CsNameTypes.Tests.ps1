<#
    A CLASS CHOSEN BY A NAME STRING (#121, `rust/fbtcore/src/rows/dispatch/nametypes.rs`): under `typesByName` in `structuregate.cs.json`,
    every call of the method links the name its parameter is handed - a constant, or a property every value of which an object in the tree
    writes (an anonymous object's slot, `arguments.member`) - to the classes the dictionary holds under it, matched as `enum_type_links`
    matches. `name_type_links` holds it, and `--map-slice` follows it from the call's file to the class's.

    Its helpers are its own - `-Only CsNameTypes` runs this suite alone.
#>

$script:CsNameTypesFiles = @{
    'Demo.csproj' = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings></PropertyGroup></Project>'
    'TemplateBase.cs' = "namespace Demo.Templates;`npublic abstract class TemplateBase`n{`n    public string Include(string name, object model) => name;`n}`n"
    'Standard/Parts.cs' = "namespace Demo.Templates.Emails.Standard;`npublic class Footer : Demo.Templates.TemplateBase { }`npublic class Header : Demo.Templates.TemplateBase { }`n"
    'Acme/Footer.cs' = "namespace Demo.Templates.Emails.Acme;`npublic class Footer : Demo.Templates.TemplateBase { }`n"
    'Body.cs' = "namespace Demo.Templates.Emails.Standard;`npublic class OrderBody : Demo.Templates.TemplateBase`n{`n" +
        "    public string Render(object model, string chosen)`n    {`n" +
        "        var direct = Include(`"Footer`", model);`n" +
        "        var parts = new[] { new { PartName = `"Header`", Object = model } };`n" +
        "        var viaProperty = string.Concat(parts.Select(p => Include(p.PartName, p.Object)));`n" +
        "        return direct + viaProperty + Include(`"Nowhere`", model) + Include(chosen, model);`n    }`n}`n"
    'cs.json' = '{ "typesByName": [ { "method": "Demo.Templates.TemplateBase.Include", "parameter": "name", "base": "Demo.Templates.TemplateBase", "namespace": "Templates.Emails", "default": ".Standard", "ignoreCase": true } ] }'
}

Test-Case 'csnametypes: a name a call hands a declared lookup links to the classes it selects (#121)' {
    $tree = Use-Tree $script:CsNameTypesFiles
    $db = Join-Path $tree 'map.sqlite'
    $run = Invoke-Gate --root $tree --ext .cs --map-sqlite $db --cs-config (Join-Path $tree 'cs.json')
    Assert-Exit $run 0
    $r = Invoke-Gate --map-query $db --width 0 --limit 0 --sql ("SELECT 'l=' || line || '|' || coalesce(value, '-') || '|' || via || '|' || kind || '|' || coalesce(prefix, '-') || '|' || " +
        "coalesce(type_symbol, '-') AS l FROM name_type_links")
    Assert-Exit $r 0
    # A LITERAL: the default class and the customer's override (`default` given, `Acme` listed nowhere).
    Assert-Line $r 'l=6|Footer|literal|default|-|Demo.Templates.Emails.Standard.Footer'
    Assert-Line $r 'l=6|Footer|literal|override|Acme|Demo.Templates.Emails.Acme.Footer'
    # A PROPERTY OF AN ANONYMOUS OBJECT the call reads back: the slot that writes it.
    Assert-Line $r 'l=8|Header|property|default|-|Demo.Templates.Emails.Standard.Header'
    # A NAME NO CLASS ANSWERS, and an argument the map cannot follow, are listed rather than dropped.
    Assert-Line $r 'l=9|Nowhere|literal|none|-|-'
    Assert-Line $r 'l=9|-|unresolved|none|-|-'
    # THE SLOT KNOWS WHAT IT FILLS: the anonymous property, as the read of it binds.
    $m = Invoke-Gate --map-query $db --width 0 --sql "SELECT 'slot=' || a.name || '|' || o.kind AS s FROM arguments a JOIN objects o ON o.id = a.object JOIN refs r ON r.symbol = a.member WHERE r.name = 'p.PartName'"
    Assert-Line $m 'slot=PartName|anonymous'
    # THE SLICE FOLLOWS IT: the call's file depends on the class its name selects.
    $slice = Join-Path $tree 'slice.sqlite'
    Assert-Exit (Invoke-Gate --map-slice $db --slice-files 'Acme/Footer.cs' --map-sqlite $slice) 0
    $in = Invoke-Gate --map-query $slice --width 0 --sql "SELECT 'in=' || path AS r FROM files WHERE sliced = 1"
    Assert-Line $in 'in=Body.cs'
}

# (#124) A METHOD OF A GENERIC CLASS: a call is stored closed (`TemplateBase<System.Object>.Include`), and the convention may name it bare or open -
# generic argument lists are ignored on both sides, at every level.
Test-Case 'csnametypes: a method declared on a generic class links its calls, however the convention spells it (#124)' {
    $tree = Use-Tree @{
        'Demo.csproj' = $script:CsNameTypesFiles['Demo.csproj']
        'TemplateBase.cs' = "namespace Demo.Services;`npublic abstract class TemplateBase<TModel>`n{`n    public TModel? Model { get; set; }`n    public string Include(string name, object? model) => name;`n}`n"
        'Standard/Parts.cs' = "namespace Demo.Templates.Emails.Standard;`npublic class Header : Demo.Services.TemplateBase<object> { }`n" +
            "public class Body : Demo.Services.TemplateBase<object>`n{`n    public string Render() => Include(`"Header`", Model);`n}`n"
        'bare.json' = '{ "typesByName": [ { "method": "Demo.Services.TemplateBase.Include", "parameter": "name", "base": "Demo.Services.TemplateBase", "namespace": "Templates.Emails", "default": ".Standard" } ] }'
        'open.json' = '{ "typesByName": [ { "method": "Demo.Services.TemplateBase<TModel>.Include", "parameter": "name", "base": "Demo.Services.TemplateBase<TModel>", "namespace": "Templates.Emails", "default": ".Standard" } ] }'
    }
    foreach ($config in 'bare.json', 'open.json') {
        $db = Join-Path $tree "$config.sqlite"
        Assert-Exit (Invoke-Gate --root $tree --ext .cs --map-sqlite $db --cs-config (Join-Path $tree $config)) 0
        $r = Invoke-Gate --map-query $db --width 0 --sql "SELECT 'g=' || line || '|' || value || '|' || via || '|' || kind || '|' || coalesce(type_symbol, '-') AS l FROM name_type_links"
        Assert-Line $r 'g=5|Header|literal|default|Demo.Templates.Emails.Standard.Header'
    }
}
