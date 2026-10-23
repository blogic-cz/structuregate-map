/**
 * ONE WALK PER BODY - and everything a body says: its calls, what it assigns, what it declares, what it
 * returns, and the branch or case each of those sits in.
 *
 * Ported from the original Angular extractor. Imports are flat: see `TsTypeRef.mjs`.
 *
 * A CALLBACK IS A FIRST-CLASS BODY. `.pipe(map((x) => {...}))`, `.then((r) => {...})`, `.sort((a, b) => ...)`
 * hold locals, assignments, calls and returns of their own, and attributing them to the enclosing method was
 * wrong twice over: the method looked like it declared things it does not, and the callback's own return
 * value - the thing a consumer follows - was recorded nowhere.
 *
 * NO PER-BODY CAP. There used to be one (300 rows), justified as protection against generated code, and
 * measured over a whole scope the largest body stayed well under it. A limit that never triggers defends
 * nothing and is one more number to defend; one that DID trigger would silently shorten a body's record.
 */
import { locationOf } from './TsNodes.mjs';
import { catchFacts, chainOf } from './TsCatch.mjs';
import { loopFacts, loopKind } from './TsLoops.mjs';

export function makeBodyRows({ ts, store, evalNode, evalCell, describe, declFileOf, declareInline, declareClass, propName, shapeAt, bindingFacts, injectAt, resolveArg, targetLine }) {
  /**
   * THE BRANCH OR CASE A STATEMENT SITS IN IS PART OF WHAT THE STATEMENT MEANS.
   *
   * `case Invoice: return 'x';` recorded as a bare `returns` row says the method returns 'x' - full stop -
   * when it returns 'x' for ONE category. Same failure as an empty guard list reading "unguarded": a
   * conditional fact published as an unconditional one.
   */
  let currentCase = null;
  let currentBranch = null;
  /**
   * THE LOOP A STATEMENT SITS IN (rows 70, `TsLoops.mjs`): the id of the innermost `loops` row whose BODY holds the row, on every row
   * that takes `branch`. Never a `branch` - those are conditions. LEXICAL (rows 71): a function or class expression in the body stays in the loop, as `branch` does.
   */
  let currentLoop = null;
  /** How many rows the emitters wrote: a bare expression statement is described only when its walk wrote none. */
  let rowsWritten = 0;
  /**
   * THE ARM OF A CONDITIONAL EXPRESSION IS A BRANCH TOO. `c ? this.t('a') : this.t('b')` runs the
   * first call only when `c` holds, and a statement row inside an arm published with only the enclosing `if`
   * and `case` read as needing neither side of `c`. Each arm is a CHOICE link, the vocabulary a value already
   * uses (`key_branches`): the id of the condition's own `expressions` row, `!`-prefixed for the arm taken when
   * it is false - `&&` runs its right side under the left, `||` under its negation, `??` under nothing a gate
   * reads. The id comes from the same cached `describe` the evaluator's `$cond_expr` uses, and
   * the walk of the conditional itself mints it (rows 71), so every conditional has a row and one the evaluator already described keeps its id.
   */
  let currentChoices = [];
  const choiceLinks = () => {
    const out = [];
    for (const { node, role, negated } of currentChoices) {
      const id = describe ? describe(node, role).id : null;
      if (id !== null && id !== undefined) out.push(negated ? `!${id}` : id);
    }
    return out;
  };
  const inCase = () => {
    const choices = currentChoices.length ? choiceLinks() : [];
    return {
      ...(currentCase === null ? {} : { case: currentCase }),
      ...(currentBranch === null ? {} : { branch: currentBranch }),
      ...(currentLoop === null ? {} : { loop: currentLoop }),
      ...(choices.length ? { choices } : {}),
    };
  };
  /** Walk `body` inside one arm: `node` is the condition, read under `negated`. */
  const underChoice = (node, role, negated, body, walk) => {
    const held = currentChoices;
    currentChoices = [...held, { node, role, negated }];
    try {
      walk(body);
    } finally {
      currentChoices = held;
    }
  };

  /** Every identifier the right-hand side reads, in source order, de-duplicated. */
  function identifiersIn(node) {
    const seen = new Set();
    const walk = (n) => {
      if (ts.isIdentifier(n)) seen.add(n.text);
      ts.forEachChild(n, walk);
    };
    walk(node);
    return [...seen];
  }

  /** The method names the right-hand side calls. */
  function calleesIn(node) {
    const seen = new Set();
    const walk = (n) => {
      if (ts.isCallExpression(n)) {
        const callee = n.expression;
        seen.add(ts.isPropertyAccessExpression(callee) ? callee.name.text : callee.getText());
      }
      ts.forEachChild(n, walk);
    };
    walk(node);
    return [...seen];
  }

  /** Every shape that owns a body and therefore its own scope. From node KINDS, not names. */
  const isFunctionLike = (node) => ts.isArrowFunction(node) || ts.isFunctionExpression(node)
    || ts.isFunctionDeclaration(node) || ts.isMethodDeclaration(node) || ts.isGetAccessor(node)
    || ts.isSetAccessor(node) || ts.isConstructorDeclaration(node);

  /** `=` and every compound/logical assignment form, from the compiler's own token range. */
  const isAssignmentOperator = (kind) => kind >= ts.SyntaxKind.FirstAssignment
    && kind <= ts.SyntaxKind.LastAssignment;

  /**
   * Classify an assignment target by NODE KIND: a class property (`this.x`), or a plain binding.
   *
   * `this.labels[key] = ...` IS an assignment, and recognising only a dotted target lost every one of them -
   * hundreds of statements produced no row at all, so a consumer asking what writes `labels` got nothing. The KEY is
   * usually dynamic, so the name is the CONTAINER being written into, which is what a consumer can join on.
   */
  function targetOf(left) {
    if (ts.isPropertyAccessExpression(left)) {
      if (left.expression.kind === ts.SyntaxKind.ThisKeyword) return { name: left.name.text, scope: 'this' };
      // A PROPERTY OF ONE OF THE CLASS'S OWN FIELDS (`this.cfg.list = ..`) names that field (`holder`, rows 44): a child
      // bound to `cfg.list` loops over what is written there.
      const holder = ts.isPropertyAccessExpression(left.expression) && left.expression.expression.kind === ts.SyntaxKind.ThisKeyword
        ? left.expression.name.text : undefined;
      return { name: left.name.text, scope: 'property', ...(holder !== undefined ? { holder } : {}) };
    }
    if (ts.isElementAccessExpression(left)) {
      const receiver = left.expression;
      if (ts.isPropertyAccessExpression(receiver)
        && receiver.expression.kind === ts.SyntaxKind.ThisKeyword) {
        return { name: receiver.name.text, scope: 'this-indexed' };
      }
      if (ts.isIdentifier(receiver)) return { name: receiver.text, scope: 'local-indexed' };
      return { name: receiver.getText(), scope: 'indexed' };
    }
    if (ts.isIdentifier(left)) return { name: left.text, scope: 'local' };
    return null;
  }

  /** `const` / `let` / `var`, from the declaration list's own flags - never from the keyword text. */
  function declarationKindOf(node) {
    const list = node.parent;
    if (!list || !ts.isVariableDeclarationList(list)) return 'unknown';
    if ((list.flags & ts.NodeFlags.Const) !== 0) return 'const';
    if ((list.flags & ts.NodeFlags.Let) !== 0) return 'let';
    return 'var';
  }

  function emitCall(node, ownerId, classId) {
    if (!ts.isCallExpression(node) && !ts.isNewExpression(node)) return;
    const callee = node.expression;
    const calleeName = ts.isPropertyAccessExpression(callee) ? callee.name : callee;
    // EVERY IN-TREE TARGET CARRIES THE `line` OF ITS DECLARATION (rows 80, 81, `TsExpr/TsReadRefs.mjs::targetLine`): the rollup binds `target_id` to the row AT that place, never to a same-named declaration of the file.
    const declared = declFileOf(calleeName);
    const target = declared !== null && targetLine !== undefined ? { ...declared, ...(targetLine(calleeName) ?? {}) } : declared;
    // EVERY argument, not the first four. A call's fifth argument is as much a fact as its first, and the
    // cut was invisible: a consumer saw a complete-looking row that silently dropped the rest.
    // A SPREAD ARGUMENT CARRIES WHAT IT SPREADS (`$spread`): `list.push(...this.make())` adds the elements `make` returns,
    // and the bare `...this.make()` text named no call a reader could enter.
    const args = [...(node.arguments ?? [])].map((a) => {
      const value = evalNode(a);
      if (!ts.isSpreadElement(a) || value === null || typeof value !== 'object' || Array.isArray(value)) return value;
      return { ...value, $spread: evalNode(a.expression) };
    });
    // A BARE ARGUMENT THAT NAMES A VARIABLE KEEPS ITS DECLARATION (rows 79, #103, `TsExpr/TsReadRefs.mjs::resolveArg`): `args` holds the const's evaluated value, and a value cannot be
    // tied back to the const and the file that declare it. One item per such argument, `position` its index in `args` - never a change to the value, which every reader takes as evaluated.
    const argRefs = [];
    [...(node.arguments ?? [])].forEach((a, position) => {
      const ref = resolveArg(a);
      if (ref !== null) argRefs.push({ position, ...ref });
    });
    rowsWritten += 1;
    const id = store.add('calls', 'cl', {
      ...inCase(),
      member: ownerId,
      callee: callee.getText(),
      method: ts.isPropertyAccessExpression(callee) ? callee.name.text : callee.getText(),
      new: ts.isNewExpression(node),
      target, args, ...(argRefs.length ? { arg_refs: argRefs } : {}), ...locationOf(node),
    });
    // A CHAINED CALL NAMES THE CALL IT IS MADE ON (`receiver_call`): `a.b(1).then(f)` is two rows starting at one
    // place, and a join on that place is ambiguous on a chain of three. The outer row is minted first - the walk
    // reaches the receiver after it - so the receiver's node is remembered until its own row exists.
    // AN `inject()` IN A FUNCTION THAT IS NO CLASS MEMBER (a functional interceptor, a provider's factory) is a `di` row of that function (rows 78, `TsExpr/TsInjects.mjs`), and one in a
    // CONSTRUCTOR body a row of the class (rows 80): `injectAt` knows which owners are constructors, and a method body gets none.
    if (injectAt && ts.isCallExpression(node)) injectAt(node, ownerId, classId);
    const outer = receiverOf.get(node);
    if (outer !== undefined) {
      store.rowOf(outer).receiver_call = id;
      receiverOf.delete(node);
    }
    let receiver = ts.isPropertyAccessExpression(callee) || ts.isElementAccessExpression(callee) ? callee.expression : null;
    while (receiver && (ts.isParenthesizedExpression(receiver) || ts.isNonNullExpression(receiver))) receiver = receiver.expression;
    if (receiver && (ts.isCallExpression(receiver) || ts.isNewExpression(receiver))) receiverOf.set(receiver, id);
  }
  const receiverOf = new WeakMap();

  /**
   * WHAT A CLASS PROPERTY IS ASSIGNED FROM - the edge that turns a template gate into a reason.
   *
   * A template gates on a component PROPERTY, never on a feature code; the class then assigns that property
   * from something meaningful. Before this table the map published both ends and not the edge: the gate knew
   * the identifier, the call knew the feature argument, the member knew the property, and nothing joined
   * them. Nothing here knows what a "feature" is - that join belongs to the consumer.
   */
  /** The target a written left side NAMES, through what only TYPES it: `(o.x as T) = v` writes `o.x`. */
  const written = (left) => {
    let at = left;
    while (ts.isParenthesizedExpression(at) || ts.isAsExpression(at) || ts.isTypeAssertionExpression(at)
      || ts.isNonNullExpression(at) || (ts.isSatisfiesExpression && ts.isSatisfiesExpression(at))) at = at.expression;
    return at;
  };

  function emitAssignment(node, ownerId, classId) {
    if (!ts.isBinaryExpression(node) || !isAssignmentOperator(node.operatorToken.kind)) return;
    const left = written(node.left);
    // A DESTRUCTURING ASSIGNMENT (`[xs[i], xs[j]] = [xs[j], xs[i]]`) writes its pattern: one row, the pattern its target.
    const pattern = ts.isArrayLiteralExpression(left) || ts.isObjectLiteralExpression(left);
    const lhs = pattern ? { name: left.getText(), scope: 'destructuring' } : targetOf(left);
    if (!lhs) return;
    const rhs = node.right;
    rowsWritten += 1;
    store.add('assignments', 'as', {
      ...inCase(),
      member: ownerId, class: classId,
      target: lhs.name, scope: lhs.scope, ...(lhs.holder !== undefined ? { holder: lhs.holder } : {}),
      // WHICH PROPERTY IS BEING SET, resolved. Published as text only, the write side of TypeScript had no
      // link where the read side has one.
      target_ref: ts.isPropertyAccessExpression(left) ? declFileOf(left.name)
        : ts.isIdentifier(left) ? declFileOf(left) : null,
      operator: ts.tokenToString(node.operatorToken.kind) ?? '=',
      value: evalCell(rhs), expression: treeOf(rhs),
      source: rhs.getText(),
      reads: identifiersIn(rhs), calls: calleesIn(rhs),
      ...locationOf(node),
    });
  }

  /**
   * `x++`, `--this.n` WRITE (rows 70): an assignment row with operator `++`/`--`, no value - what it leaves is not the value of any expression.
   * Without it a counter bumped only by an update had no writer in the map.
   */
  function emitUpdate(node, ownerId, classId) {
    if (!ts.isPrefixUnaryExpression(node) && !ts.isPostfixUnaryExpression(node)) return;
    if (node.operator !== ts.SyntaxKind.PlusPlusToken && node.operator !== ts.SyntaxKind.MinusMinusToken) return;
    const operand = written(node.operand);
    const lhs = targetOf(operand);
    if (!lhs) return;
    rowsWritten += 1;
    store.add('assignments', 'as', {
      ...inCase(),
      member: ownerId, class: classId,
      target: lhs.name, scope: lhs.scope, ...(lhs.holder !== undefined ? { holder: lhs.holder } : {}),
      target_ref: ts.isPropertyAccessExpression(operand) ? declFileOf(operand.name)
        : ts.isIdentifier(operand) ? declFileOf(operand) : null,
      operator: ts.tokenToString(node.operator), value: null, expression: null,
      source: node.getText(), reads: identifiersIn(operand), calls: calleesIn(operand),
      ...locationOf(node),
    });
  }

  /**
   * A LOOP IS A `loops` ROW, and its BODY's rows carry its id as `loop` (rows 70, `TsLoops.mjs`). The header - initializer, condition,
   * incrementor, iterable - is walked in the ENCLOSING context: a call in it runs once, or per turn, but is not the body's.
   * `expr` is the header expression's tree, the id of its `expressions` row, as an `if`'s `condition_expr` is. `source` is '' for `for (;;)`.
   */
  function collectLoop(node, ownerId, classId, walk) {
    const facts = loopFacts(ts, node);
    const id = store.add('loops', 'lp', {
      ...inCase(),
      member: ownerId, class: classId, kind: facts.kind, ...(facts.await ? { await: true } : {}),
      variables: facts.variables,
      source: facts.header ? facts.header.getText() : '',
      expr: facts.header && describe ? describe(facts.header, 'loop').id ?? null : null,
      ...(facts.incrementor ? { incrementor_source: facts.incrementor.getText() } : {}),
      ...locationOf(node),
    });
    for (const part of [node.initializer, node.condition, node.incrementor, node.expression]) if (part) walk(part);
    const held = currentLoop;
    currentLoop = id;
    try {
      walk(node.statement);
    } finally {
      currentLoop = held;
    }
  }

  /** A BARE EXPRESSION STATEMENT whose walk wrote no row (`this.a.b;`, `x;`, `a && b;`) is described, so no statement of the source is absent. */
  function collectStatement(node, walk) {
    const before = rowsWritten;
    ts.forEachChild(node, walk);
    if (rowsWritten === before && describe) describe(node.expression, 'statement');
  }

  /**
   * LOCAL DECLARATIONS inside a body - the sibling of `assignments`, for values never assigned to a member.
   *
   * A DESTRUCTURING declaration keeps its pattern: `const { a, b } = f()` records the bindings AND the
   * shared initializer, so a consumer can pair binding `a` with property `a` of whatever `f` returns.
   * Nothing here infers that pairing - it records the two halves and lets the join happen where the
   * question is asked.
   */
  function emitLocal(node, ownerId, classId) {
    // A DECLARATION WITHOUT AN INITIALIZER IS A LOCAL TOO: `let t: string;`, a loop's `for (const x of xs)`, a `catch (e)` -
    // each names a value a later read or call uses, and the map had none of them.
    if (!ts.isVariableDeclaration(node)) return;
    const init = node.initializer;
    const bindings = [];
    if (ts.isIdentifier(node.name)) bindings.push({ property: null, local: node.name.text });
    else {
      // EACH NAME THE PATTERN BINDS IS ONE ITEM, AT ANY DEPTH (rows 78): `{ A: { B: { C: c } }, A: a }` binds `c` and `a`, with `property` the PATH to the name (`A.B.C`) - a nested
      // pattern is no binding of its own, and one item per outer element hid every name inside it. Each keeps its default, its type and - in an array pattern - its position (rows 77,
      // `TsExpr/TsBindings.mjs`): a hole (`[, x]`) is no binding, yet `x` is the second element. An element of an ARRAY has no key, so the path stops at the array - a top-level
      // array element has `property` null and its `index` (rows 80; rows 78 kept the name it binds, against its own contract).
      const bound = (pattern, path) => {
        const array = ts.isArrayBindingPattern(pattern);
        pattern.elements.forEach((el, at) => {
          if (!ts.isBindingElement(el)) return;
          const key = el.propertyName ? propName(el.propertyName) : (ts.isIdentifier(el.name) ? el.name.text : null);
          const here = array ? path : [...path, key];
          if (!ts.isIdentifier(el.name)) { bound(el.name, here); return; }
          const sf = el.getSourceFile();
          const where = sf.getLineAndCharacterOfPosition(el.name.getStart());
          const property = array ? (path.length ? path.join('.') : null) : here.join('.');
          bindings.push({ property, local: el.name.text, ...bindingFacts(el, array ? at : null), line: where.line + 1, col: where.character + 1 });
        });
      };
      bound(node.name, []);
    }
    rowsWritten += 1;
    store.add('locals', 'lo', {
      ...inCase(),
      member: ownerId, class: classId,
      name: ts.isIdentifier(node.name) ? node.name.text : node.name.getText(),
      destructured: !ts.isIdentifier(node.name),
      bindings,
      declared: declarationKindOf(node),
      type: node.type ? node.type.getText() : null,
      ...(init ? {
        value: evalCell(init), expression: treeOf(init), source: init.getText(),
        reads: identifiersIn(init), calls: calleesIn(init),
      } : {}),
      ...locationOf(node),
    });
  }

  /**
   * THE TREE OF WHAT IS RETURNED OR BOUND, as the id of its `expressions` row - on `returns` and `locals`.
   *
   * `value` is the evaluator's answer, and for a CALL CHAIN that answer is the last link alone:
   * `ids.map((i) => this.active.includes(i)).some((e) => e)` keeps `.some` and its argument, and the `.map`
   * that says what is being tested is reachable only as text. The tree keeps every link. Null where the
   * expression is a bare read chain, which the summary columns already state in full.
   */
  const treeOf = (node) => (node && describe ? describe(node, 'expr').id ?? null : null);

  /** RETURN VALUES - what a function actually produces. A function with several returns gets several rows;
   *  that is the truth of a branching function, and collapsing them would invent a single answer. */
  function emitReturn(node, ownerId, classId) {
    if (!ts.isReturnStatement(node)) return;
    const value = node.expression ? evalCell(node.expression) : null;
    rowsWritten += 1;
    store.add('returns', 'rv', {
      ...inCase(),
      member: ownerId, class: classId,
      value, expression: treeOf(node.expression),
      source: node.expression ? node.expression.getText() : null,
      reads: node.expression ? identifiersIn(node.expression) : [],
      calls: node.expression ? calleesIn(node.expression) : [],
      ...locationOf(node),
    });
  }

  /** WHAT IS THROWN, under the branch or case it sits in - the `raises` row the other halves write. */
  function emitThrow(node, ownerId, classId) {
    if (!ts.isThrowStatement(node)) return;
    const thrown = node.expression;
    rowsWritten += 1;
    store.add('raises', 'th', {
      ...inCase(),
      member: ownerId, class: classId,
      name: ts.isNewExpression(thrown) ? chainOf(ts, thrown.expression) : chainOf(ts, thrown),
      value: evalCell(thrown), expression: treeOf(thrown),
      source: thrown.getText(),
      reads: identifiersIn(thrown), calls: calleesIn(thrown),
      ...locationOf(node),
    });
  }

  /**
   * A `try` IS A BRANCH NO STATEMENT SITS IN. Its block runs unconditionally until it throws, and a catch
   * block is "maybe", which the closure has no vocabulary for: so `currentBranch` is not moved for any of
   * the three blocks, and nothing inside them gains a `branch` because of the try. Its catch clause is a
   * `handlers` row, as in every other half.
   */
  function collectTry(node, ownerId, classId) {
    store.add('branches', 'br', {
      member: ownerId, class: classId, parent: currentBranch, sense: 'try',
      condition: null, condition_expr: null, condition_source: '', ...locationOf(node),
    });
    const handler = catchFacts(ts, node.getSourceFile(), node);
    if (handler) {
      store.add('handlers', 'hd', {
        ...inCase(), member: ownerId, class: classId, ...handler, ...locationOf(node.catchClause),
      });
    }
  }

  /**
   * A CASE GROUP, not a clause: `case A: case B: return x;` puts the return in B while A is empty, so one
   * row per clause would make a consumer walk backwards to discover A leads there too.
   *
   * The DISCRIMINANT travels with it, because `case Invoice` is meaningless until you know whether the
   * switch is on `categoryId` or `kind`.
   */
  function collectSwitch(node, ownerId, classId, walk) {
    const discriminant = evalCell(node.expression);
    // THE TREES, as for an `if`: the discriminant's row is one per SWITCH, so it also says which cases are
    // siblings - which a `default` needs, since it is every value its siblings do not name.
    const treeFor = (n, role) => (describe ? describe(n, role).id ?? null : null);
    const discriminantExpr = treeFor(node.expression, 'switch');
    walk(node.expression); // calls inside the discriminant belong to the ENCLOSING case
    const parent = currentCase;
    let labels = [];
    let labelExprs = [];
    let sawDefault = false;
    for (const clause of node.caseBlock.clauses) {
      if (ts.isDefaultClause(clause)) sawDefault = true;
      else {
        labels.push(evalNode(clause.expression));
        labelExprs.push(treeFor(clause.expression, 'case'));
        // A LABEL CAN CONTAIN CODE, and walking clauses by hand stops `forEachChild` from ever reaching it -
        // several labels lost their `calls` rows silently. Evaluated in the ENCLOSING scope, before any case.
        walk(clause.expression);
      }
      // An empty clause falls through: its labels belong to the next clause that has statements.
      if (!clause.statements.length) continue;
      const caseId = store.add('switch_cases', 'sc', {
        member: ownerId, class: classId, parent,
        discriminant, discriminant_expr: discriminantExpr, discriminant_source: node.expression.getText(),
        labels, label_exprs: labelExprs, is_default: sawDefault, ...locationOf(clause),
      });
      currentCase = caseId;
      // RESTORED IN `finally`: this state lives in a closure built once per program, so an exception while
      // walking one clause would leave the case set for every file extracted afterwards.
      try {
        for (const st of clause.statements) walk(st);
      } finally {
        currentCase = parent;
      }
      labels = [];
      labelExprs = [];
      sawDefault = false;
    }
  }

  /**
   * A STATEMENT INSIDE AN `if` IS AS CONDITIONAL AS ONE INSIDE A `case` - at three times the volume.
   *
   * `sense` is what a case row does not need: an `else` branch means the condition is FALSE, and without it
   * a consumer reading the chain would invert half of it. `else if` needs no special case - the inner `if`'s
   * row parents to the outer `if`'s ELSE row, so walking `parent` up yields "B and not A" by construction.
   */
  function collectIf(node, ownerId, classId, walk) {
    const condition = evalCell(node.expression);
    const source = node.expression.getText();
    // THE CONDITION'S TREE, always. `condition` is the evaluator's answer, which drops the tree of a bare
    // read and keeps only the last link of a call: `LIST.includes(this.model.level)` came back as a
    // `$call` naming `includes`, and nothing said which list or which value it tested.
    const tree = describe ? describe(node.expression, 'branch').id ?? null : null;
    const parent = currentBranch;
    walk(node.expression); // the condition is evaluated in the ENCLOSING branch, not inside itself
    const branch = (sense, body) => {
      const id = store.add('branches', 'br', {
        member: ownerId, class: classId, parent, sense,
        condition, condition_expr: tree, condition_source: source, ...locationOf(body),
      });
      currentBranch = id;
      try {
        walk(body);
      } finally {
        currentBranch = parent;
      }
    };
    branch('then', node.thenStatement);
    if (node.elseStatement) branch('else', node.elseStatement);
  }

  /** The body of an arrow that has no block: the expression IS what the arrow returns. */
  const isConciseBody = (node) => !ts.isBlock(node) && node.parent !== undefined
    && ts.isArrowFunction(node.parent) && node.parent.body === node;

  function collectBody(bodyNode, ownerId, classId, skip = null) {
    if (!bodyNode) return;
    // A CONCISE ARROW BODY IS THE RETURN VALUE, and it may itself contain functions. Without this, every
    // concise arrow returned nothing in the map. Emitted HERE, for every owner, and not only for a callback:
    // an arrow PROPERTY (`check = (id) => [...].some(...)`) and an arrow `const` are walked straight from
    // their declaration, and once published only as the member's `$fn` value - no `returns` row - the call
    // to one restricted nothing where a method with the same body did.
    if (isConciseBody(bodyNode)) {
      store.add('returns', 'rv', {
        ...inCase(),
        member: ownerId, class: classId, implicit: true,
        value: evalCell(bodyNode), expression: treeOf(bodyNode), source: bodyNode.getText(),
        reads: identifiersIn(bodyNode), calls: calleesIn(bodyNode), ...locationOf(bodyNode),
      });
    }
    // A CONCISE BODY THAT IS ITSELF A FUNCTION - `(a) => (b) => ..` - is that function: its own row and walk, never folded into
    // the outer one's.
    if (isConciseBody(bodyNode) && isFunctionLike(bodyNode)) {
      const inlineId = declareInline(bodyNode, ownerId);
      if (bodyNode.body) collectBody(bodyNode.body, inlineId, classId);
      defaults(bodyNode, inlineId, classId);
      return;
    }
    const walk = (node) => {
      if (skip !== null && node === skip.node) { skip.walk(); return; } // its own declaration's row, its body walked HERE: in the branch, loop and case it stands in
      if (node !== bodyNode && isFunctionLike(node)) {
        const inlineId = declareInline(node, ownerId);
        rowsWritten += 1;
        // A `loop` IS LEXICAL, AS `branch` IS (rows 71): a callback in a loop body runs in that loop, and a call there read as outside it.
        const fnBody = node.body;
        if (fnBody) collectBody(fnBody, inlineId, classId);
        defaults(node, inlineId, classId);
        return; // its contents belong to IT, not to the body being walked
      }
      // A CLASS EXPRESSION IS A CLASS: `static Inner = class { static A = 'a'; }` - its own row and members, under its holder's name.
      if (declareClass && ts.isClassExpression(node)) {
        declareClass(node); // a loop around it holds it, as a branch does
        return;
      }
      if (ts.isSwitchStatement(node)) { collectSwitch(node, ownerId, classId, walk); return; }
      if (ts.isIfStatement(node)) { collectIf(node, ownerId, classId, walk); return; }
      if (loopKind(ts, node) !== null) { collectLoop(node, ownerId, classId, walk); return; }
      if (ts.isExpressionStatement(node)) { collectStatement(node, walk); return; }
      if (ts.isTryStatement(node)) collectTry(node, ownerId, classId);
      // The condition is walked where it stands; each arm under its side of it. Same roles as the evaluator's.
      // EVERY CONDITIONAL HAS ITS `expressions` ROW (rows 71), minted here by the same cached `describe` the arms' `choices` and the
      // evaluator's `$cond_expr` use: one minted only when a row sat under an arm left 516 of 19220 with none.
      if (ts.isConditionalExpression(node)) {
        if (describe) describe(node.condition, 'ternary');
        walk(node.condition);
        underChoice(node.condition, 'ternary', false, node.whenTrue, walk);
        underChoice(node.condition, 'ternary', true, node.whenFalse, walk);
        return;
      }
      const logic = ts.isBinaryExpression(node) ? node.operatorToken.kind : null;
      if (describe && (logic === ts.SyntaxKind.AmpersandAmpersandToken || logic === ts.SyntaxKind.BarBarToken
        || logic === ts.SyntaxKind.QuestionQuestionToken)) describe(node.left, 'logic');
      if (logic === ts.SyntaxKind.AmpersandAmpersandToken || logic === ts.SyntaxKind.BarBarToken) {
        walk(node.left);
        underChoice(node.left, 'logic', logic === ts.SyntaxKind.BarBarToken, node.right, walk);
        return;
      }
      // WHERE AN OBJECT LITERAL STANDS is its `shapes` row's context (rows 71, `TsExpr/TsShapes.mjs`).
      if (shapeAt && ts.isObjectLiteralExpression(node)) shapeAt(node, { member: ownerId, class: classId, ...inCase() });
      emitCall(node, ownerId, classId);
      emitAssignment(node, ownerId, classId);
      emitUpdate(node, ownerId, classId);
      emitLocal(node, ownerId, classId);
      emitReturn(node, ownerId, classId);
      emitThrow(node, ownerId, classId);
      ts.forEachChild(node, walk);
    };
    walk(bodyNode);
    if (skip !== null) skip.walk(); // a function the walk never reached still has its body walked (idempotent)
  }

  /** A parameter's default value is code like any other: `f(cond = () => true)` holds a function. */
  function defaults(fn, ownerId, classId) {
    for (const p of fn.parameters ?? []) if (p.initializer) collectExpr(p.initializer, ownerId, classId);
  }

  /**
   * AN EXPRESSION NO BODY HOLDS, WALKED AS ONE (1:1 with the source): a property initializer, a decorator's argument, a top-level
   * statement, a parameter default. Its calls, assignments and locals are rows under `ownerId`, and a function in it - the
   * expression itself when it is one - is a `functions` row with its own body, as inside a method. They were the code a
   * consumer asking "who calls X" could not see: a callback in `items$ = src.pipe(map((v) => ..))` and the call itself.
   */
  function collectExpr(node, ownerId, classId, skip = null) {
    if (!node) return;
    if (isFunctionLike(node)) {
      const inlineId = declareInline(node, ownerId);
      if (node.body) collectBody(node.body, inlineId, classId);
      defaults(node, inlineId, classId);
      return;
    }
    collectBody(node, ownerId, classId, skip);
  }

  return { collectBody, collectExpr, defaults };
}
