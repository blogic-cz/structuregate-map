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
