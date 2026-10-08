//! ONE STYLESHEET, READ INTO ITS DECLARATIONS - every one under the selector it really applies to.
//!
//! `raffia` parses; this walks what it built. Nothing here looks at the text for structure: the text is only
//! SLICED, at the spans the parser gave, so a selector or a value is quoted exactly as it was written.
//!
//! WHAT A ROW IS: one declaration under one RESOLVED selector. SCSS nesting is what makes the second half of
//! that worth a column - `.legend-item { &--hidden { display: none } }` declares nothing about `&--hidden`, it
//! declares `.legend-item--hidden` - so the selector is resolved the way Sass resolves it: every `&` is the
//! parent selector, and a nested selector without one is a DESCENDANT of it. A parent LIST multiplies: inside
//! `.a, .b`, `&.x` is two rows. What wraps the rule is kept too, because `display: none` inside
//! `@media (max-width: 768px)` hides an element only on a narrow screen: the `@media` conditions in `media`, every
//! other at-rule (`@include`, `@supports`, `@if`, ...) in `context`.
//!
//! A RULE THAT IS NEVER EMITTED WHERE IT IS WRITTEN is a row with `live = false`: inside a `@mixin` (or a Less
//! mixin) it applies wherever it is included, and a `%placeholder` only where it is extended. Neither is
//! followed - that would be a Sass compile, with every `@use` resolved - so the row says what it is instead.

use raffia::ast::{
    AtRule, AtRulePrelude, ComplexSelector, ComplexSelectorChild, ComponentValue, CompoundSelector, Declaration,
    InterpolableIdent, PseudoClassSelectorArgKind, SassAtRootKind, SelectorList, SimpleBlock, SimpleSelector,
    Statement, Stylesheet,
};
use raffia::{Parser, Spanned, Syntax};

/// One declaration under one resolved selector.
#[derive(Debug, Clone)]
pub struct Decl {
    pub line: usize,
    pub col: usize,
    /// The line of the selector it sits under, which is not the declaration's own on a rule of many lines.
    pub rule_line: usize,
    /// The complex selector AS WRITTEN - `&--hidden`.
    pub selector: String,
    /// ...and as Sass emits it - `.legend-item--hidden`.
    pub resolved: String,
    pub media: Option<String>,
    pub context: Option<String>,
    pub property: String,
    pub value: String,
    pub important: bool,
    /// `display: none`, `visibility: hidden` or `visibility: collapse`, read off the parsed value: a variable or a
    /// function there is NOT a hide, because what it evaluates to is not written here.
    pub hides: bool,
    pub live: bool,
}

/// What one file said, or why it said nothing.
#[derive(Debug, Default)]
pub struct Sheet {
    pub decls: Vec<Decl>,
    /// The parser's own message and the line it stopped at. A sheet that did not parse has NO rows: a parser
    /// that gave up half way has not said what the other half declares.
    pub error: Option<(String, usize)>,
    /// What the parser recovered from and kept going - counted, because the rows are still its reading.
    pub recovered: usize,
}

/// The syntax a file is read in, by its extension - or none, when it is not a stylesheet.
pub fn syntax_of(ext: &str) -> Option<Syntax> {
    match ext {
        "css" => Some(Syntax::Css),
        "scss" => Some(Syntax::Scss),
        "sass" => Some(Syntax::Sass),
        "less" => Some(Syntax::Less),
        _ => None,
    }
}

/// At-rules whose block styles no ELEMENT: descriptors (`@font-face`, `@page`, `@property`), animation frames,
/// and a Sass function body. Their declarations would read as rules of whatever encloses them.
const NOT_ELEMENT_STYLES: &[&str] = &[
    "font-face", "page", "property", "counter-style", "font-feature-values", "font-palette-values",
    "position-try", "function", "view-transition", "color-profile",
];

#[derive(Clone)]
struct Target {
    written: String,
    resolved: String,
    line: usize,
    placeholder: bool,
}

#[derive(Clone)]
struct Ctx {
    targets: Vec<Target>,
    media: Vec<String>,
    context: Vec<String>,
    live: bool,
}

/// Whitespace runs as one space, ends trimmed: a selector split over lines is still one selector.
pub fn squeeze(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for word in text.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}

/// The byte offset of every `&` in a selector, its pseudo-class arguments included (`:not(&--x)`).
pub fn nesting_offsets(selector: &ComplexSelector, out: &mut Vec<usize>) {
    for child in &selector.children {
        if let ComplexSelectorChild::CompoundSelector(compound) = child {
            compound_nesting(compound, out);
        }
    }
}

fn compound_nesting(compound: &CompoundSelector, out: &mut Vec<usize>) {
    for simple in &compound.children {
        match simple {
            SimpleSelector::Nesting(nesting) => out.push(nesting.span.start),
            SimpleSelector::PseudoClass(pseudo) => match pseudo.arg.as_ref().map(|a| &a.kind) {
                Some(PseudoClassSelectorArgKind::SelectorList(list)) => {
                    list.selectors.iter().for_each(|s| nesting_offsets(s, out))
                }
                Some(PseudoClassSelectorArgKind::CompoundSelector(c)) => compound_nesting(c, out),
                Some(PseudoClassSelectorArgKind::CompoundSelectorList(list)) => {
                    list.selectors.iter().for_each(|c| compound_nesting(c, out))
                }
                Some(PseudoClassSelectorArgKind::RelativeSelectorList(list)) => {
                    list.selectors.iter().for_each(|r| nesting_offsets(&r.complex_selector, out))
                }
                _ => {}
            },
            _ => {}
        }
    }
}

fn has_placeholder(selector: &ComplexSelector) -> bool {
    selector.children.iter().any(|child| match child {
        ComplexSelectorChild::CompoundSelector(c) => {
            c.children.iter().any(|s| matches!(s, SimpleSelector::SassPlaceholder(_)))
        }
        ComplexSelectorChild::Combinator(_) => false,
    })
}

struct Walk<'s> {
    src: &'s str,
    starts: Vec<usize>,
    out: Vec<Decl>,
}

impl<'s> Walk<'s> {
    fn new(src: &'s str) -> Self {
        let mut starts = vec![0];
        starts.extend(src.char_indices().filter(|(_, c)| *c == '\n').map(|(i, _)| i + 1));
        Walk { src, starts, out: Vec::new() }
    }

    fn slice(&self, start: usize, end: usize) -> &'s str {
        self.src.get(start..end).unwrap_or("")
    }

    /// 1-based line and column (in characters) of a byte offset.
    fn pos(&self, offset: usize) -> (usize, usize) {
        let line = self.starts.partition_point(|&s| s <= offset);
        let start = self.starts[line.saturating_sub(1)];
        (line, self.slice(start, offset).chars().count() + 1)
    }

    /// A selector resolved against its parents. `bare` is what `@at-root` does to a selector with no `&`: it
    /// stays at the root rather than becoming a descendant.
    fn resolve(&self, selector: &ComplexSelector, parents: &[Target], bare: bool) -> Vec<(String, bool)> {
        let written = self.slice(selector.span.start, selector.span.end);
        let own = has_placeholder(selector);
        let mut amps = Vec::new();
        nesting_offsets(selector, &mut amps);
        amps.retain(|&a| self.slice(a, a + 1) == "&");
        amps.sort_unstable();
        if parents.is_empty() || (amps.is_empty() && bare) {
            return vec![(squeeze(written), own)];
        }
        parents
            .iter()
            .map(|parent| {
                let text = if amps.is_empty() {
                    format!("{} {}", parent.resolved, written)
                } else {
                    let mut text = String::new();
                    let mut at = selector.span.start;
                    for &amp in &amps {
                        text.push_str(self.slice(at, amp));
                        text.push_str(&parent.resolved);
                        at = amp + 1;
                    }
                    text.push_str(self.slice(at, selector.span.end));
                    text
                };
                (squeeze(&text), own || parent.placeholder)
            })
            .collect()
    }

    fn targets(&self, list: &SelectorList, parents: &[Target], bare: bool) -> Vec<Target> {
        let mut out = Vec::new();
        for selector in &list.selectors {
            let written = squeeze(self.slice(selector.span.start, selector.span.end));
            let line = self.pos(selector.span.start).0;
            for (resolved, placeholder) in self.resolve(selector, parents, bare) {
                out.push(Target { written: written.clone(), resolved, line, placeholder });
            }
        }
        out
    }

    fn rule(&mut self, list: &SelectorList, block: &SimpleBlock, ctx: &Ctx, bare: bool) {
        let mut inner = ctx.clone();
        inner.targets = self.targets(list, &ctx.targets, bare);
        self.block(&block.statements, &inner);
    }

    fn block(&mut self, statements: &[Statement], ctx: &Ctx) {
        for statement in statements {
            match statement {
                Statement::QualifiedRule(rule) => self.rule(&rule.selector, &rule.block, ctx, false),
                Statement::LessConditionalQualifiedRule(rule) => {
                    let mut inner = ctx.clone();
                    inner.context.push(format!("when {}", squeeze(self.slice(rule.guard.span.start, rule.guard.span.end))));
                    self.rule(&rule.selector, &rule.block, &inner, false);
                }
                Statement::Declaration(decl) => self.decl(decl, ctx),
                Statement::AtRule(at) => self.at_rule(at, ctx),
                Statement::SassIfAtRule(rule) => {
                    let mut clauses = vec![("@if", Some(&rule.if_clause.condition), &rule.if_clause.block)];
                    clauses.extend(rule.else_if_clauses.iter().map(|c| ("@else if", Some(&c.condition), &c.block)));
                    if let Some(block) = &rule.else_clause {
                        clauses.push(("@else", None, block));
                    }
                    for (word, condition, block) in clauses {
                        let mut inner = ctx.clone();
                        inner.context.push(match condition {
                            Some(c) => format!("{word} {}", squeeze(self.slice(c.span().start, c.span().end))),
                            None => word.to_string(),
                        });
                        self.block(&block.statements, &inner);
                    }
                }
                Statement::UnknownSassAtRule(at) => {
                    if let Some(block) = &at.block {
                        let mut inner = ctx.clone();
                        inner.context.push(squeeze(self.slice(at.span.start, block.span.start)));
                        self.block(&block.statements, &inner);
                    }
                }
                Statement::LessMixinDefinition(mixin) => {
                    let mut inner = ctx.clone();
                    inner.targets = Vec::new();
                    inner.live = false;
                    inner.context.push(format!("mixin {}", squeeze(self.slice(mixin.span.start, mixin.block.span.start))));
                    self.block(&mixin.block.statements, &inner);
                }
                _ => {}
            }
        }
    }

    fn at_rule(&mut self, at: &AtRule, ctx: &Ctx) {
        let Some(block) = &at.block else { return };
        let name = at.name.name.to_ascii_lowercase();
        if name.ends_with("keyframes") || NOT_ELEMENT_STYLES.contains(&name.as_str()) {
            return;
        }
        let prelude = at.prelude.as_ref().map(|p| squeeze(self.slice(p.span().start, p.span().end)));
        let mut inner = ctx.clone();
        match (name.as_str(), &at.prelude) {
            ("media", _) => inner.media.push(prelude.unwrap_or_default()),
            ("at-root", Some(AtRulePrelude::SassAtRoot(root))) => {
                if let SassAtRootKind::Selector(list) = &root.kind {
                    return self.rule(list, block, ctx, true);
                }
                inner.targets = Vec::new();
                inner.context.push(format!("@at-root {}", prelude.unwrap_or_default()));
            }
            ("at-root", _) => inner.targets = Vec::new(),
            ("nest", Some(AtRulePrelude::Nest(list))) => return self.rule(list, block, ctx, false),
            ("mixin", _) => {
                inner.targets = Vec::new();
                inner.live = false;
                inner.context.push(format!("@mixin {}", prelude.unwrap_or_default()));
            }
            _ => inner.context.push(match prelude {
                Some(p) if !p.is_empty() => format!("@{name} {p}"),
                _ => format!("@{name}"),
            }),
        }
        self.block(&block.statements, &inner);
    }

    fn decl(&mut self, decl: &Declaration, ctx: &Ctx) {
        if ctx.targets.is_empty() {
            return;
        }
        let (property, literal) = match &decl.name {
            InterpolableIdent::Literal(id) if id.name.starts_with("--") => (id.name.to_string(), true),
            InterpolableIdent::Literal(id) => (id.name.to_ascii_lowercase(), true),
            other => (squeeze(self.slice(other.span().start, other.span().end)), false),
        };
        let values: Vec<&ComponentValue> =
            decl.value.iter().filter(|v| !matches!(v, ComponentValue::ImportantAnnotation(_))).collect();
        let value = match (values.first(), values.last()) {
            (Some(first), Some(last)) => squeeze(self.slice(first.span().start, last.span().end)),
            _ => String::new(),
        };
        let word = match values.as_slice() {
            [ComponentValue::InterpolableIdent(InterpolableIdent::Literal(id))] => Some(id.name.to_ascii_lowercase()),
            _ => None,
        };
        let hides = literal
            && matches!(
                (property.as_str(), word.as_deref()),
                ("display", Some("none")) | ("visibility", Some("hidden")) | ("visibility", Some("collapse"))
            );
        let important = decl.important.is_some()
            || decl.value.iter().any(|v| matches!(v, ComponentValue::ImportantAnnotation(_)));
        let (line, col) = self.pos(decl.span.start);
        let join = |parts: &[String], by: &str| (!parts.is_empty()).then(|| parts.join(by));
        for target in &ctx.targets {
            self.out.push(Decl {
                line,
                col,
                rule_line: target.line,
                selector: target.written.clone(),
                resolved: target.resolved.clone(),
                media: join(&ctx.media, " and "),
                context: join(&ctx.context, " > "),
                property: property.clone(),
                value: value.clone(),
                important,
                hides,
                live: ctx.live && !target.placeholder,
            });
        }
    }
}

/// One stylesheet's text, read in `syntax`. A byte-order mark is not part of the sheet.
pub fn read(text: &str, syntax: Syntax) -> Sheet {
    let src = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut walk = Walk::new(src);
    let mut parser = Parser::new(src, syntax);
    let parsed = parser.parse::<Stylesheet>();
    let recovered = parser.recoverable_errors().len();
    match parsed {
        Ok(sheet) => {
            let ctx = Ctx { targets: Vec::new(), media: Vec::new(), context: Vec::new(), live: true };
            walk.block(&sheet.statements, &ctx);
            Sheet { decls: walk.out, error: None, recovered }
        }
        Err(error) => Sheet { decls: Vec::new(), error: Some((error.kind.to_string(), walk.pos(error.span.start).0)), recovered },
    }
}
