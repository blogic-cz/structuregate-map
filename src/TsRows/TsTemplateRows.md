# Template rows that follow Angular, not the written name (rows 70)

Four template rows said more, or less, than the compiler does. Each is read off the stored rows by `TsTpl/`; the case for all four is
`tests/deep/TsAngular/TsResolved/TsRowsTemplateRows.Tests.ps1`.

- **A `*dir` wrapper is matched by its `templateAttrs` alone** (`TsTpl/TsTplMatch.mjs`): the selector is the compiler's OWN
  `createCssSelectorFromNode` - a `Template` is `ng-template`, and when its `tagName` is not (a desugared `*` wrapper) only its
  `templateAttrs` are attributes, else its attributes (no i18n one), Property/TwoWay inputs and outputs. An element takes only Property/TwoWay
  inputs (`[class.x]`, `[attr.x]`, `[style.x]` select nothing). So `<button ripple *demoIf>` renders `RippleDirective` once, on the element, a
  component never sits on a wrapper, and `element_unmatched` counts an unmatched element once (the wrapper is not noted). `childOwnedBindings`
  stays: it still keeps a wrapper from emitting its child's binding rows twice.
- **An accessor pair is read by its getter and written by its setter** (`TsTpl/TsTplReads.mjs`, `propByOwner`/`setterByOwner`): whichever was
  declared last used to win both. The member a `Write` heads takes the setter; every other hop the getter.
- **A chain resolves from any value, not only the component's context** (`TsTplReads.mjs::valueOf`, `resolveChain`): past a keyed read (the
  ELEMENT of what it indexes), a call (the method's `return_ref`, new on `members` rows of methods and function-valued properties, shaped as a
  `type_ref`; `TsDecls/TsClassRows.mjs::returnRefOf`), a pipe (an in-tree pipe's `transform` `return_ref`, else a framework pipe's `unwrapped`)
  and `a ?? b`, `a || b`, `c ? a : b` when both sides name ONE owner.
- **A template's own condition is read in the OUTER scope** (`TsTplReads.mjs::scopeAt(.., outer)`): the `templateAttrs`/inputs of a `Template`
  and an `@if` condition are evaluated before the names they bind exist, so `*ngIf="x$ | async as x$"` reads the member `x$`; its children see the alias.
- **The consumer contracts these rows keep** (`type_ref`, `return_ref`, normalised `calls.args`, template writes, `i18nCarriers`) are written in `skills/map-sqlite/typescript-refs.md`; a change to one updates it.
- **A loop variable, an `as` alias and every chain go through ONE value** (rows 72, `TsTplReads.mjs::loopOwnerOf`/`aliasOwnerOf` over `valueOf`; the owners are
  `TsTplOwners.mjs`, split out): `ownerRefOfChain` took the type of the FIRST name met anywhere in the collection, so a call typed its variable with an
  argument's list and an un-annotated method (`list() { return this.all; }`) typed nothing. A value keeps its `ref` with no owner (an array's owner is `Array`, no
  row), because the element of the `ref` is what a loop takes. Over a pipe: an in-tree `transform`'s return, whose element it is, else a framework pipe's `unwrapped`.
  Case: `tests/deep/TsAngular/TsValues/TsRowsReadSources.Tests.ps1`.
- **A type parameter is read through its bounds** (`TsTypeRef.mjs::boundsOf`, `TsTplOwners.mjs::paramOwner`): in an instantiation the argument passed, the `default`
  where none was (`NOT_PASSED`; an argument that names nothing is `null`, never the default), outside one - the generic class's own template - the `constraint`.
  A generic alias has `type_params` and its `args` are `aliasTypeArguments`; an `element`/`unwrapped` that is a parameter is the whole `param` ref.
- **An interface has its bases and a ref its dropped literals** (`interfaces.extends_refs`; `type_ref.shapes`): `propWithEnv` walks bases nearest first over a visited
  set and answers the ONE row an `any` owner's parts agree on; `X & { Extra?: E }` is the `any` owner of `X` and the literal. A UNION's literal is not a member.
- **A literal holds what it spreads, an alias written stands in for a library type** (`TsTplOwners.mjs`, `spreadsByShape`, `ref.alias`): `{ ...x, column: 1 }` answers the
  names of `x`'s type, the last spread first, once the literal's own names miss; `type Desc = Readonly<{ .. }>` is `Readonly` to the checker, so the ref's `alias` names `Desc`,
  whose computed members answer (the library type's own arguments are never the alias's).
- **An intersection is ALL its parts, a union ONE of its arms** (rows 72, `type_refs_of`, `TsTplOwners.mjs::propWithEnv`): `x: A & { id: number }` over `class A { id }` is a
  property both declare - ONE symbol to the checker, whose first declaration is the part WRITTEN first - so the `any` owner of the parts is `and` and answers with that part's row
  (`{ id } & A` the literal's), and the cursor goes on as an `and` of what every declaring part types it as. A `union` (`or`) answers only when its arms agree on ONE declaration;
  alias rows that each carry the generic's own member (`HolderA | HolderB`, both `Holder<..>.Input`) are one place, so one. Where an arm's type is unknown the chain ends.
- **What names no declaration still has members** (rows 72): a mapped type is a ref with `props`, the declarations its kept members come from, so `Pick<Dto, 'a'>` reaches `a` and no
  other name; a literal in a generic class takes the class's bindings (`Items: { Row: TItem }[]`); `this['a'][0]['b']` is its constraint; `T extends A | B` is the `any` of both.
- **A name the template binds is a VALUE** (rows 73, `TsTplReads.mjs::scopeAt`, `elementRefOf`): a loop variable, an `as` alias and a `#ref` keep the `{owner, ref, env}` `valueOf` answers, so a loop over `X[][]`'s
  variable takes ITS element, to any depth (the element of an array of arrays is the first of the ref's `args`, whole). A `#ref` is the instance of the COMPONENT matched on its element, or - `#r="name"` - of the
  matched directive whose `exportAs` lists `name` (a comma list); a plain element (the DOM node) and `<ng-template>` (`TemplateRef`) type nothing, yet shadow the member. It is visible in its whole scope - siblings
  and earlier nodes too - up to the nearest enclosing `ng-template` or block.

- **A mapped type a declared alias names is that alias** (rows 73, `TsTplOwners.mjs::ownerOf`): `type T = Pick<Slot<unknown>, 'k'>` reads through the alias's own computed members, which carry the types the
  checker instantiated (a parameter left to its default included); a ref's `props` answer an anonymous mapping only, being the source's declared rows with no bindings. A keyed read over an `as` alias of a
  framework pipe takes the element of the list the observable emits (`wrapped`, one level in), not of its innermost type. Case: `TsValues/TsRowsScopeValues.Tests.ps1`.

- **A template is typed by the directive it meets** (rows 74, `TsTpl/TsTplContext.mjs`): the variables of an `<ng-template>` or a `*dir` wrapper are the properties of the context the matched in-tree directive declares - `members.guard_ref` of its static
  `ngTemplateContextGuard<U>(dir: X<U>, ..): ctx is Ctx<U>` (`TsDecls/TsClassRows.mjs::memberRow`), else the `TemplateRef<Ctx>` it injects (`di`) - a bare `let-x` or `let x` being `$implicit`, `let-x="p"` the property `p`, `*dir="v as x"` the property `dir`.
  The directive's type parameters are bound from the inputs on the node (`io` names the member, `feedOf`): `T`, `T[]` (the element of `wrapped` first) or `Box<T>`; `dir`'s own arguments say which of the guard's parameters is which. An `ngFor`/`ngIf` has no
  rows and keeps the special reading. A `#ref` to a generic COMPONENT is its instance with the same binding (`refValueOf`, memoised per ref, the inputs read in the element's scope with no ref typed, so two refs never ask each other); a parameter nothing feeds
  is its `default`, else its `constraint`. A loop over an `as` alias of a framework pipe takes the element of `wrapped` (`loopValueOf`), and so does a loop over the pipe itself (rows 77): `let row of (grid$ | async)` over `Observable<X[][]>` is `X[]`, never the innermost `X` of `unwrapped`. `ownerOfRow` resolves each part of an intersection or union with the env it was reached in, so a literal part reads `T` as the
  instantiation does. Case: `tests/deep/TsAngular/TsValues/TsRowsTemplateTypes.Tests.ps1`. `template_nodes.directives` is read by it, so a partial run is no longer handed it back as a stub (`TsSetup/TsCarried.mjs`).

- **A context built in code types the variables** (rows 75, `TsDecls/TsExpr/TsViewContext.mjs`, `TsTpl/TsTplContext.mjs`): `inject(TemplateRef)` and `TemplateRef<any>` declare no context, yet `createEmbeddedView(this.tpl, { rule: this.rule })` hands
  the view one. `di.context_ref` is the `type_ref` of that second argument (a shape ref for an object literal or a literal-typed local) on the `di` row of the `TemplateRef` the call renders - found by the declaration the
  checker resolved (`createEmbeddedView` in `@angular/core`) and the declaration the first argument names, only when every call of the class hands ONE type - and the `let-x` / `as x` variables read it where the guard and the declared
  `TemplateRef<Ctx>` say nothing. A directive with no context of its own takes its BASE class's (`baseTypedOf`, the base's parameters bound from `extends`). `inject(TemplateRef)` in a property initializer makes a directive STRUCTURAL as the constructor parameter does (rows 77): its `di` row carries the `type_ref` of the call's type (`TsClassRows.mjs::isAngularTemplateRef`, `TsRegistry.mjs`). Case: `tests/deep/TsAngular/TsValues/TsRowsTemplateTypes.Tests.ps1`.
