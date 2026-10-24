<#
    A CLASS CHOSEN BY AN ENUM MEMBER'S NAME (#113, `rust/fbtcore/src/rows/dispatch/enumtypes.rs`): under a convention declared in
    `structuregate.cs.json` (`typesByEnumName`, read by `rows/dispatch/csconfig.rs`), every member of the enum links to the classes
    deriving from the base - through the classes in between - in the namespace, named as the member; a class in a `prefixes`
    namespace is an override. `enum_type_links` holds it, and `--map-slice` follows it from a file referencing the member.

    Its helpers are its own - `-Only CsEnumTypes` runs this suite alone.
#>

$script:CsEnumTypesProject = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>'
$script:CsEnumTypesFiles = @{
    'Demo.csproj' = $script:CsEnumTypesProject
    'ReportIDs.cs' = "namespace Demo.Core;`npublic enum ReportIDs { SalesSummary, StockList, Ghost }`n"
    'ReportBase.cs' = "namespace Demo.Templates;`npublic abstract class ReportBase { }`n"
    'Reports/SalesSummary.cs' = "namespace Demo.Templates.Reports;`npublic class SalesSummary : Demo.Templates.ReportBase { }`n"
    'Reports/Pdf/SalesSummary.cs' = "namespace Demo.Templates.Reports.Pdf;`npublic class SalesSummary : Demo.Templates.ReportBase { }`n"
    'Reports/Mid.cs' = "namespace Demo.Templates.Reports;`npublic abstract class MidBase : Demo.Templates.ReportBase { }`npublic class StockList : MidBase { }`n"
    'Other/StockList.cs' = "namespace Demo.Other;`npublic class StockList : Demo.Templates.ReportBase { }`n"
    'Sender.cs' = "namespace Demo.Core;`npublic class Sender`n{`n    public object Send() => Pick(ReportIDs.SalesSummary);`n    private object Pick(ReportIDs id) => id;`n}`n"
    'cs.json' = '{ "typesByEnumName": [ { "enum": "Demo.Core.ReportIDs", "base": "Demo.Templates.ReportBase", "namespace": "Templates.Reports", "prefixes": ["Pdf"] }, { "enum": "Demo.Core.Missing", "base": "Demo.Templates.ReportBase" } ] }'
}

function Get-CsEnumTypesRows([string]$Db, [string]$Sql) {
    $r = Invoke-Gate --map-query $Db --width 0 --sql $Sql
    Assert-Exit $r 0
    return $r
}

Test-Case 'csenumtypes: an enum member links to the classes a declared convention selects by its name' {
    $tree = Use-Tree $script:CsEnumTypesFiles
    $db = Join-Path $tree 'map.sqlite'
    $run = Invoke-Gate --root $tree --ext .cs --map-sqlite $db --cs-config (Join-Path $tree 'cs.json')
    Assert-Exit $run 0
    $r = Get-CsEnumTypesRows $db "SELECT 'l=' || member || '|' || kind || '|' || coalesce(prefix, '-') || '|' || coalesce(type_symbol, '-') AS l FROM enum_type_links"
    Assert-Line $r 'l=SalesSummary|default|-|Demo.Templates.Reports.SalesSummary'
    Assert-Line $r 'l=SalesSummary|override|Pdf|Demo.Templates.Reports.Pdf.SalesSummary'
    # THROUGH A CLASS IN BETWEEN; a same-named class OUTSIDE the namespace is not it; a member nothing answers is listed, not dropped.
    Assert-Line $r 'l=StockList|default|-|Demo.Templates.Reports.StockList'
    Assert-NoLine $r 'Demo.Other.StockList'
    Assert-Line $r 'l=Ghost|none|-|-'
    # A CONVENTION THAT MATCHES NOTHING is said.
    Assert-Line $run 'typesByEnumName: no enum Demo.Core.Missing in the map'
    # THE SLICE FOLLOWS IT: a file referencing the member depends on the class its name selects.
    $slice = Join-Path $tree 'slice.sqlite'
    Assert-Exit (Invoke-Gate --map-slice $db --slice-files 'Reports/SalesSummary.cs' --map-sqlite $slice) 0
    Assert-Line (Get-CsEnumTypesRows $slice "SELECT 'in=' || path AS r FROM files WHERE sliced = 1") 'in=Sender.cs'
}

# (#113 follow-up) `default` GIVEN: a class in a namespace holding `.Standard` is the default, every other an override keyed by its last
# namespace segment - customers need not be listed. `Acme` is listed nowhere and is an override all the same.
Test-Case 'csenumtypes: with a default namespace every other class is an override by its last segment' {
    $tree = Use-Tree @{
        'Demo.csproj' = $script:CsEnumTypesProject
        'Ids.cs' = "namespace Demo.Core;`npublic enum MailIDs { Welcome }`n"
        'Base.cs' = "namespace Demo.Mail;`npublic abstract class MailBase { }`n"
        'Standard.cs' = "namespace Demo.Templates.Emails.Standard;`npublic class Welcome : Demo.Mail.MailBase { }`n"
        'Acme.cs' = "namespace Demo.Templates.Emails.Acme;`npublic class Welcome : Demo.Mail.MailBase { }`n"
        'cs.json' = '{ "typesByEnumName": [ { "enum": "Demo.Core.MailIDs", "base": "Demo.Mail.MailBase", "namespace": "Templates.Emails", "default": ".Standard" } ] }'
    }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .cs --map-sqlite $db --cs-config (Join-Path $tree 'cs.json')) 0
    $r = Get-CsEnumTypesRows $db "SELECT 'l=' || member || '|' || kind || '|' || coalesce(prefix, '-') || '|' || type_symbol AS l FROM enum_type_links"
    Assert-Line $r 'l=Welcome|default|-|Demo.Templates.Emails.Standard.Welcome'
    Assert-Line $r 'l=Welcome|override|Acme|Demo.Templates.Emails.Acme.Welcome'
}

# (#115) THROUGH A GENERIC BASE: `ReportBase<TModel>`'s symbol keeps its parameter and its subclass names it closed (`ReportBase<object>`); compared without
# their arguments, `SalesSummary` derives from `Demo.Mail.MailBase` through it. `StockList` derives from the base directly.
Test-Case 'csenumtypes: a class deriving through a generic base links' {
    $tree = Use-Tree @{
        'Demo.csproj' = $script:CsEnumTypesProject
        'Ids.cs' = "namespace Demo.Core;`npublic enum ReportIDs { SalesSummary, StockList }`n"
        'Base.cs' = "namespace Demo.Mail;`npublic abstract class MailBase { }`n"
        'ReportBase.cs' = "namespace Demo.Templates;`npublic class ReportBase<TModel> : Demo.Mail.MailBase { }`n"
        'Sales.cs' = "namespace Demo.Templates.Reports;`npublic class SalesSummary : Demo.Templates.ReportBase<object> { }`n"
        'Stock.cs' = "namespace Demo.Templates.Reports;`npublic class StockList : Demo.Mail.MailBase { }`n"
        'cs.json' = '{ "typesByEnumName": [ { "enum": "Demo.Core.ReportIDs", "base": "Demo.Mail.MailBase", "namespace": "Templates.Reports" } ] }'
    }
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .cs --map-sqlite $db --cs-config (Join-Path $tree 'cs.json')) 0
    $r = Get-CsEnumTypesRows $db "SELECT 'l=' || member || '|' || kind || '|' || coalesce(type_symbol, '-') AS l FROM enum_type_links"
    Assert-Line $r 'l=SalesSummary|default|Demo.Templates.Reports.SalesSummary'
    Assert-Line $r 'l=StockList|default|Demo.Templates.Reports.StockList'
}

# (#115) A BASE THE COMPILER COULD NOT BIND (no reference to its assembly) is stored as its bare name: never matched to the configured base, but said.
Test-Case 'csenumtypes: an unresolved base links nothing and the note says it is unresolved' {
    $tree = Use-Tree @{
        'Demo.csproj' = $script:CsEnumTypesProject
        'Ids.cs' = "namespace Demo.Core;`npublic enum ReportIDs { StockList }`n"
        'Stock.cs' = "using Lib.Widgets;`nnamespace Demo.Templates.Reports;`npublic class StockList : WidgetBase { }`n"
        'cs.json' = '{ "typesByEnumName": [ { "enum": "Demo.Core.ReportIDs", "base": "Lib.Widgets.WidgetBase", "namespace": "Templates.Reports" } ] }'
    }
    $db = Join-Path $tree 'map.sqlite'
    $run = Invoke-Gate --root $tree --ext .cs --map-sqlite $db --cs-config (Join-Path $tree 'cs.json')
    Assert-Line $run '1 class(es) name an UNRESOLVED base `WidgetBase`'
    Assert-Line (Get-CsEnumTypesRows $db "SELECT 'l=' || member || '|' || kind AS l FROM enum_type_links") 'l=StockList|none'
}

# NO CONVENTION, NO LINK: equal names alone link nothing, and the table is empty.
Test-Case 'csenumtypes: without a declared convention equal names link nothing' {
    $files = $script:CsEnumTypesFiles.Clone()
    $files.Remove('cs.json')
    $tree = Use-Tree $files
    $db = Join-Path $tree 'map.sqlite'
    Assert-Exit (Invoke-Gate --root $tree --ext .cs --map-sqlite $db) 0
    Assert-Line (Get-CsEnumTypesRows $db "SELECT 'n=' || count(*) AS n FROM enum_type_links") 'n=0'
}
