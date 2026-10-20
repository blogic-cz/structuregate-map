# C# branch conditions

`branches.condition` is the test as a TREE of node rows (`branches__condition`, `owner` = the branch id), in ROSLYN'S shape:
  `kind` is the syntax kind (`EqualsExpression`, `LogicalAndExpression`, `LogicalNotExpression`, `IsPatternExpression`,
  `InvocationExpression`, `IdentifierName`, `SimpleMemberAccessExpression`), `source` its text, `op` the operator, and operands sit
  under `left`/`right`, `operand`, `pattern` (an `is`; its `OrPattern`/`AndPattern`/`NotPattern`/`ConstantPattern` nodes carry `op`,
  `left`/`right`/`operand` and the constant's facts), `condition`/`then`/`else` (`?:`), `receiver` and `args` (a call). A name,
  member access or call also carries `symbol` (what it binds to, `Demo.Tier.Gold`) and, when folded, `const`/`const_kind`
  (`enum`, `const`, `folded`, `symbol`). Parentheses are no node and a dotted access (`Store.Ready`) is one leaf. A `try`, or a
  `for` without a test, has no nodes. C# has NO `condition_expr`: nothing is evaluated, the tree is `condition` itself.
  `SELECT b.line FROM branches b JOIN branches__condition r ON r.owner = b.id AND r.role = 'symbol' AND r.value = 'Demo.Tier.Gold'`.
