# The C# map: which code runs where the call names no class

Two derived tables of the deep map (`--map-sqlite`), each rewritten whole after every half. The rest of the C# tables are
[csharp-sql.md](csharp-sql.md).

* `dispatch` (derived after every half, rewritten whole): which handler a mediator call runs - `call` joins `calls.id`,
  `file`, `line`, `func`, `kind` (`request` for a `Send`, `notification` for a `Publish`), `message` (the argument's
  STATIC type), `handler` (`<class>.Handle`), `handler_file`, `handler_line`. One row per class whose closed base
  `IRequestHandler<T, ..>` / `INotificationHandler<T>` names exactly that type - a message typed by an interface or a
  base class, or with no handler, has none.
* `enum_type_links` (derived after every half, rewritten whole, #113): a class chosen at run time by an ENUM MEMBER'S NAME (a
  dictionary filled from `Type.Name` by reflection, read with `member.ToString()`) - only under a convention declared in
  `structuregate.cs.json` (`--cs-config`): `typesByEnumName: [{enum, base, namespace, prefixes, default, ignoreCase}]`. One row per member
  and class: `enum_symbol`, `member`, `member_symbol` (joins `refs.symbol`), `kind` (`default`; `override` for a class whose last
  namespace segment is in `prefixes` - or, with `default` (`".Standard"`) given, every class whose namespace does not hold it, its last segment the prefix; `none` for a member no class answers), `type_symbol`, `type_file`, `type_line`. The class
  derives from `base` through any classes between; its namespace holds `namespace`. Equal names without a convention link nothing.

A slice (`--map-slice`) follows `enum_type_links`: a file that references a member (`refs.symbol`) keeps the class files its name
selects as dependents.
