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

export function makeBodyRows({ ts, store, evalNode, describe, declFileOf, declareInline, propName }) {
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
   * THE ARM OF A CONDITIONAL EXPRESSION IS A BRANCH TOO. `c ? this.t('a') : this.t('b')` runs the
   * first call only when `c` holds, and a statement row inside an arm published with only the enclosing `if`
   * and `case` read as needing neither side of `c`. Each arm is a CHOICE link, the vocabulary a value already
   * uses (`key_branches`): the id of the condition's own `expressions` row, `!`-prefixed for the arm taken when
   * it is false - `&&` runs its right side under the left, `||` under its negation, `??` under nothing a gate
   * reads. The id is minted LAZILY, by the same cached `describe` the evaluator's `$cond_expr` comes from, so a
   * condition no row sits under writes no row, and one the evaluator already described keeps its id.
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
      return { name: left.name.text, scope: 'property' };
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

  function emitCall(node, ownerId) {
    if (!ts.isCallExpression(node) && !ts.isNewExpression(node)) return;
    const callee = node.expression;
    const target = declFileOf(ts.isPropertyAccessExpression(callee) ? callee.name : callee);
    // EVERY argument, not the first four. A call's fifth argument is as much a fact as its first, and the
    // cut was invisible: a consumer saw a complete-looking row that silently dropped the rest.
    const args = [...(node.arguments ?? [])].map((a) => evalNode(a));
    store.add('calls', 'cl', {
      ...inCase(),
      member: ownerId,
      callee: callee.getText(),
      method: ts.isPropertyAccessExpression(callee) ? callee.name.text : callee.getText(),
      new: ts.isNewExpression(node),
      target, args, ...locationOf(node),
    });
  }

  /**
   * WHAT A CLASS PROPERTY IS ASSIGNED FROM - the edge that turns a template gate into a reason.
   *
   * A template gates on a component PROPERTY, never on a feature code; the class then assigns that property
   * from something meaningful. Before this table the map published both ends and not the edge: the gate knew
   * the identifier, the call knew the feature argument, the member knew the property, and nothing joined
   * them. Nothing here knows what a "feature" is - that join belongs to the consumer.
   */
  function emitAssignment(node, ownerId, classId) {
    if (!ts.isBinaryExpression(node) || !isAssignmentOperator(node.operatorToken.kind)) return;
    const lhs = targetOf(node.left);
    if (!lhs) return;
    const rhs = node.right;
    store.add('assignments', 'as', {
      ...inCase(),
      member: ownerId, class: classId,
      target: lhs.name, scope: lhs.scope,
      // WHICH PROPERTY IS BEING SET, resolved. Published as text only, the write side of TypeScript had no
      // link where the read side has one.
      target_ref: ts.isPropertyAccessExpression(node.left) ? declFileOf(node.left.name)
        : ts.isIdentifier(node.left) ? declFileOf(node.left) : null,
      operator: ts.tokenToString(node.operatorToken.kind) ?? '=',
      value: evalNode(rhs), expression: treeOf(rhs),
      source: rhs.getText(),
      reads: identifiersIn(rhs), calls: calleesIn(rhs),
      ...locationOf(node),
    });
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
    if (!ts.isVariableDeclaration(node) || !node.initializer) return;
    const bindings = [];
    if (ts.isIdentifier(node.name)) bindings.push({ property: null, local: node.name.text });
    else {
      for (const el of node.name.elements) {
        if (ts.isBindingElement(el)) {
          bindings.push({
            property: el.propertyName ? propName(el.propertyName)
              : (ts.isIdentifier(el.name) ? el.name.text : null),
            local: el.name.getText(),
          });
        }
      }
    }
    store.add('locals', 'lo', {
      ...inCase(),
      member: ownerId, class: classId,
      name: ts.isIdentifier(node.name) ? node.name.text : node.name.getText(),
      destructured: !ts.isIdentifier(node.name),
      bindings,
      declared: declarationKindOf(node),
      type: node.type ? node.type.getText() : null,
      value: evalNode(node.initializer), expression: treeOf(node.initializer),
      source: node.initializer.getText(),
      reads: identifiersIn(node.initializer), calls: calleesIn(node.initializer),
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
    const value = node.expression ? evalNode(node.expression) : null;
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
    store.add('raises', 'th', {
      ...inCase(),
      member: ownerId, class: classId,
      name: ts.isNewExpression(thrown) ? chainOf(ts, thrown.expression) : chainOf(ts, thrown),
      value: evalNode(thrown), expression: treeOf(thrown),
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
    const discriminant = evalNode(node.expression);
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
    const condition = evalNode(node.expression);
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

  function collectBody(bodyNode, ownerId, classId) {
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
        value: evalNode(bodyNode), expression: treeOf(bodyNode), source: bodyNode.getText(),
        reads: identifiersIn(bodyNode), calls: calleesIn(bodyNode), ...locationOf(bodyNode),
      });
    }
    const walk = (node) => {
      if (node !== bodyNode && isFunctionLike(node)) {
        const inlineId = declareInline(node, ownerId);
        const fnBody = node.body;
        if (fnBody) collectBody(fnBody, inlineId, classId);
        return; // its contents belong to IT, not to the body being walked
      }
      if (ts.isSwitchStatement(node)) { collectSwitch(node, ownerId, classId, walk); return; }
      if (ts.isIfStatement(node)) { collectIf(node, ownerId, classId, walk); return; }
      if (ts.isTryStatement(node)) collectTry(node, ownerId, classId);
      // The condition is walked where it stands; each arm under its side of it. Same roles as the evaluator's.
      if (ts.isConditionalExpression(node)) {
        walk(node.condition);
        underChoice(node.condition, 'ternary', false, node.whenTrue, walk);
        underChoice(node.condition, 'ternary', true, node.whenFalse, walk);
        return;
      }
      const logic = ts.isBinaryExpression(node) ? node.operatorToken.kind : null;
      if (logic === ts.SyntaxKind.AmpersandAmpersandToken || logic === ts.SyntaxKind.BarBarToken) {
        walk(node.left);
        underChoice(node.left, 'logic', logic === ts.SyntaxKind.BarBarToken, node.right, walk);
        return;
      }
      emitCall(node, ownerId);
      emitAssignment(node, ownerId, classId);
      emitLocal(node, ownerId, classId);
      emitReturn(node, ownerId, classId);
      emitThrow(node, ownerId, classId);
      ts.forEachChild(node, walk);
    };
    walk(bodyNode);
  }

  return { collectBody };
}
