//! Source-to-source instrumentor for Rust code.
//!
//! Parses Rust source files using `syn`, walks the AST to find branch points
//! (`if`, `match`, `while`/`for`) and external function calls, then emits
//! instrumented source code using `quote` that calls into `shatter_rust_runtime`.
//!
//! ## Instrumentation strategy
//!
//! - **Branch points**: For each `if`/`while`/`for` condition, wrap the condition
//!   in a closure that records the branch decision via `shatter_rust_runtime::branch_hit`.
//!   For `match` arms, insert a `branch_hit` call at the top of each arm body.
//!
//! - **External calls**: For function calls to symbols not defined in the file,
//!   wrap them in a `shatter_rust_runtime::mock_call` check that can intercept
//!   and replace return values.

use std::path::Path;

use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::spanned::Spanned;
use syn::visit_mut::VisitMut;
use syn::{Expr, ExprForLoop, ExprIf, ExprMatch, ExprWhile, Stmt, parse_file};

use crate::timing::TimingCollector;

/// Errors that can occur during instrumentation.
#[derive(Debug)]
pub enum InstrumentError {
    FileNotFound(String),
    ReadError(String),
    ParseError(String),
}

impl std::fmt::Display for InstrumentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FileNotFound(p) => write!(f, "file not found: {p}"),
            Self::ReadError(e) => write!(f, "failed to read file: {e}"),
            Self::ParseError(e) => write!(f, "failed to parse file: {e}"),
        }
    }
}

/// Result of instrumenting a source file.
#[derive(Debug)]
pub struct InstrumentResult {
    /// The instrumented source code as a string.
    pub source: String,
    /// Number of branch points instrumented.
    pub branch_count: u32,
}

/// Instrument a Rust source file, optionally targeting a single function.
///
/// Returns the instrumented source code and the number of branches found.
pub fn instrument_file(
    path: &Path,
    function_name: Option<&str>,
) -> Result<InstrumentResult, InstrumentError> {
    instrument_file_with_timing(path, function_name, None)
}

pub fn instrument_file_with_timing(
    path: &Path,
    function_name: Option<&str>,
    mut timing: Option<&mut TimingCollector>,
) -> Result<InstrumentResult, InstrumentError> {
    if !path.exists() {
        return Err(InstrumentError::FileNotFound(path.display().to_string()));
    }

    let source = if let Some(timing) = timing.as_deref_mut() {
        timing.record("instrument.read", |_| {
            std::fs::read_to_string(path).map_err(|e| InstrumentError::ReadError(e.to_string()))
        })?
    } else {
        std::fs::read_to_string(path).map_err(|e| InstrumentError::ReadError(e.to_string()))?
    };

    instrument_source_with_timing(&source, function_name, timing)
}

/// Instrument Rust source code from a string.
pub fn instrument_source(
    source: &str,
    function_name: Option<&str>,
) -> Result<InstrumentResult, InstrumentError> {
    instrument_source_with_timing(source, function_name, None)
}

pub fn instrument_source_with_timing(
    source: &str,
    function_name: Option<&str>,
    mut timing: Option<&mut TimingCollector>,
) -> Result<InstrumentResult, InstrumentError> {
    let mut syntax = if let Some(timing) = timing.as_deref_mut() {
        timing.record("instrument.parse", |_| {
            parse_file(source).map_err(|e| InstrumentError::ParseError(e.to_string()))
        })?
    } else {
        parse_file(source).map_err(|e| InstrumentError::ParseError(e.to_string()))?
    };

    let (output, branch_count) = if let Some(timing) = timing.as_mut() {
        timing.record("instrument.transform", |_| {
            let mut visitor = Instrumentor::new(function_name);
            visitor.visit_file_mut(&mut syntax);
            (syntax.to_token_stream().to_string(), visitor.branch_id)
        })
    } else {
        let mut visitor = Instrumentor::new(function_name);
        visitor.visit_file_mut(&mut syntax);
        (syntax.to_token_stream().to_string(), visitor.branch_id)
    };

    Ok(InstrumentResult {
        source: output,
        branch_count,
    })
}

/// AST visitor that rewrites branch conditions and external calls.
struct Instrumentor {
    /// Next branch ID to assign.
    branch_id: u32,
    /// Next loop ID to assign.
    loop_id: u32,
    /// If set, only instrument this function.
    target_function: Option<String>,
    /// Whether we are currently inside the target function.
    inside_target: bool,
    /// Approximate line number tracker (updated as we traverse).
    current_line: u32,
}

impl Instrumentor {
    fn new(target_function: Option<&str>) -> Self {
        Self {
            branch_id: 0,
            loop_id: 0,
            target_function: target_function.map(String::from),
            inside_target: target_function.is_none(), // if no target, always active
            current_line: 0,
        }
    }

    /// Allocate and return the next branch ID.
    fn next_branch_id(&mut self) -> u32 {
        let id = self.branch_id;
        self.branch_id += 1;
        id
    }

    /// Allocate and return the next loop ID.
    fn next_loop_id(&mut self) -> u32 {
        let id = self.loop_id;
        self.loop_id += 1;
        id
    }

    /// Get the line number for an expression using its span.
    fn line_of(&self, expr: &impl syn::spanned::Spanned) -> u32 {
        let span = expr.span();
        let start = span.start();
        // proc_macro2 line numbers are 1-based when span-locations feature is enabled
        if start.line > 0 {
            start.line as u32
        } else {
            self.current_line
        }
    }

    /// Get the line number for a statement using its span.
    fn line_of_stmt(&self, stmt: &Stmt) -> u32 {
        let span = stmt.span();
        let start = span.start();
        if start.line > 0 {
            start.line as u32
        } else {
            self.current_line
        }
    }

    /// Create a branch_hit wrapper expression that evaluates the condition,
    /// records the branch, and returns the condition value.
    ///
    /// Generates:
    /// ```ignore
    /// {
    ///     let __shatter_cond = <condition>;
    ///     shatter_rust_runtime::branch_hit(<id>, <line>, __shatter_cond, <constraint_json>);
    ///     __shatter_cond
    /// }
    /// ```
    fn wrap_condition(&mut self, cond: &Expr) -> Expr {
        let id = self.next_branch_id();
        let line = self.line_of(cond);
        let constraint_json = constraint_for_expr(cond);

        let tokens: TokenStream = quote! {
            {
                let __shatter_cond = #cond;
                shatter_rust_runtime::branch_hit(#id, #line, __shatter_cond, #constraint_json);
                __shatter_cond
            }
        };

        syn::parse2(tokens).unwrap_or_else(|_| cond.clone())
    }

    /// Create a branch_hit statement for a match arm body.
    ///
    /// Generates:
    /// ```ignore
    /// shatter_rust_runtime::branch_hit(<id>, <line>, true, <constraint_json>);
    /// ```
    fn branch_hit_stmt(&mut self, line: u32, constraint_json: &str) -> Stmt {
        let id = self.next_branch_id();
        let tokens: TokenStream = quote! {
            shatter_rust_runtime::branch_hit(#id, #line, true, #constraint_json);
        };

        syn::parse2(tokens).unwrap_or_else(|_| {
            // Fallback: empty statement
            syn::parse2(quote! { ; }).expect("semicolon should parse")
        })
    }

    fn line_hit_stmt(&self, line: u32) -> Stmt {
        let tokens: TokenStream = quote! {
            shatter_rust_runtime::line_hit(#line);
        };
        syn::parse2(tokens)
            .unwrap_or_else(|_| syn::parse2(quote! { ; }).expect("semicolon should parse"))
    }

    fn loop_enter_stmt(&mut self) -> Stmt {
        let id = self.next_loop_id();
        let tokens: TokenStream = quote! {
            shatter_rust_runtime::loop_enter(#id);
        };
        syn::parse2(tokens)
            .unwrap_or_else(|_| syn::parse2(quote! { ; }).expect("semicolon should parse"))
    }
}

impl VisitMut for Instrumentor {
    fn visit_block_mut(&mut self, node: &mut syn::Block) {
        if !self.inside_target {
            return;
        }

        let mut instrumented = Vec::with_capacity(node.stmts.len() * 2);
        for mut stmt in std::mem::take(&mut node.stmts) {
            syn::visit_mut::visit_stmt_mut(self, &mut stmt);
            if should_record_stmt_line(&stmt) {
                let line = self.line_of_stmt(&stmt);
                if line > 0 {
                    instrumented.push(self.line_hit_stmt(line));
                }
            }
            instrumented.push(stmt);
        }
        node.stmts = instrumented;
    }

    fn visit_item_fn_mut(&mut self, node: &mut syn::ItemFn) {
        let fn_name = node.sig.ident.to_string();
        let was_inside = self.inside_target;

        if let Some(ref target) = self.target_function {
            if &fn_name == target {
                self.inside_target = true;
            } else {
                // Skip functions that aren't our target
                return;
            }
        } else {
            self.inside_target = true;
        }

        if node.sig.constness.is_some() {
            self.inside_target = was_inside;
            return;
        }

        let line = self.line_of(node);
        node.block.stmts.insert(0, self.line_hit_stmt(line));

        // Visit the function body
        syn::visit_mut::visit_item_fn_mut(self, node);

        self.inside_target = was_inside;
    }

    fn visit_impl_item_fn_mut(&mut self, node: &mut syn::ImplItemFn) {
        let fn_name = node.sig.ident.to_string();
        let was_inside = self.inside_target;

        if let Some(ref target) = self.target_function {
            if &fn_name == target {
                self.inside_target = true;
            } else {
                return;
            }
        } else {
            self.inside_target = true;
        }

        if node.sig.constness.is_some() {
            self.inside_target = was_inside;
            return;
        }

        let line = self.line_of(node);
        node.block.stmts.insert(0, self.line_hit_stmt(line));

        syn::visit_mut::visit_impl_item_fn_mut(self, node);

        self.inside_target = was_inside;
    }

    fn visit_expr_if_mut(&mut self, node: &mut ExprIf) {
        if !self.inside_target {
            return;
        }

        // `if let PAT = EXPR { ... }` and let-chains (`if let PAT = EXPR && cond { ... }`)
        // cannot be instrumented by wrapping the condition: `let` is only valid directly
        // inside an `if`/`while` head, and pattern bindings need to stay in scope for the
        // then-branch. Instead, insert branch_hit statements at the top of each branch
        // body where the bindings (if any) are live.
        if contains_let_expr(&node.cond) {
            let line = self.line_of(&node.cond);
            let constraint_json = format!(
                r#"{{"kind":"unknown","hint":"if let: {}"}}"#,
                escape_json_string(&node.cond.to_token_stream().to_string()),
            );
            let then_hit = self.branch_hit_stmt(line, &constraint_json);
            node.then_branch.stmts.insert(0, then_hit);

            if let Some((_, else_expr)) = &mut node.else_branch {
                let else_constraint = format!(
                    r#"{{"kind":"unknown","hint":"if let (else): {}"}}"#,
                    escape_json_string(&node.cond.to_token_stream().to_string()),
                );
                let else_hit = self.branch_hit_stmt(line, &else_constraint);
                insert_stmt_into_else(else_expr, else_hit);
            }

            // Recurse so nested ifs / matches inside the bodies are still instrumented.
            syn::visit_mut::visit_expr_if_mut(self, node);
            return;
        }

        // Wrap the condition
        let wrapped = self.wrap_condition(&node.cond);
        *node.cond = wrapped;

        // Continue visiting the then/else branches
        syn::visit_mut::visit_expr_if_mut(self, node);
    }

    fn visit_expr_while_mut(&mut self, node: &mut ExprWhile) {
        if !self.inside_target {
            return;
        }

        if contains_let_expr(&node.cond) {
            let line = self.line_of(&node.cond);
            let constraint_json = format!(
                r#"{{"kind":"unknown","hint":"while let: {}"}}"#,
                escape_json_string(&node.cond.to_token_stream().to_string()),
            );
            let branch_hit = self.branch_hit_stmt(line, &constraint_json);
            let loop_enter = self.loop_enter_stmt();
            node.body.stmts.insert(0, branch_hit);
            node.body.stmts.insert(0, loop_enter);

            syn::visit_mut::visit_expr_while_mut(self, node);
            return;
        }

        let wrapped = self.wrap_condition(&node.cond);
        *node.cond = wrapped;
        let loop_enter = self.loop_enter_stmt();
        node.body.stmts.insert(0, loop_enter);

        syn::visit_mut::visit_expr_while_mut(self, node);
    }

    fn visit_expr_for_loop_mut(&mut self, node: &mut ExprForLoop) {
        if !self.inside_target {
            return;
        }

        // For loops don't have a boolean condition, but we can record
        // entry into the loop body as a branch point
        let line = self.line_of(node);
        let constraint_json = format!(
            r#"{{"kind":"unknown","hint":"for loop over {}"}}"#,
            node.expr.to_token_stream()
        );
        let stmt = self.branch_hit_stmt(line, &constraint_json);
        let loop_enter = self.loop_enter_stmt();
        node.body.stmts.insert(0, stmt);
        node.body.stmts.insert(0, loop_enter);

        syn::visit_mut::visit_expr_for_loop_mut(self, node);
    }

    fn visit_expr_match_mut(&mut self, node: &mut ExprMatch) {
        if !self.inside_target {
            return;
        }

        let match_expr_tokens = node.expr.to_token_stream().to_string();

        for arm in &mut node.arms {
            let line = self.line_of(&arm.pat);
            let pattern_str = arm.pat.to_token_stream().to_string();
            let constraint_json = format!(
                r#"{{"kind":"bin_op","op":"eq","left":{{"kind":"param","name":"{}"}},"right":{{"kind":"const","type":"str","value":"{}"}}}}"#,
                escape_json_string(&match_expr_tokens),
                escape_json_string(&pattern_str),
            );

            let id = self.next_branch_id();
            let branch_line = line;

            // We need to wrap the arm body to include the branch_hit call.
            // The arm body is an Expr — wrap it in a block if needed.
            let body = &arm.body;
            let tokens: TokenStream = quote! {
                {
                    shatter_rust_runtime::branch_hit(#id, #branch_line, true, #constraint_json);
                    #body
                }
            };

            if let Ok(new_body) = syn::parse2::<Expr>(tokens) {
                *arm.body = new_body;
            }
        }

        syn::visit_mut::visit_expr_match_mut(self, node);
    }
}

fn should_record_stmt_line(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Local(_) | Stmt::Macro(_) => true,
        Stmt::Expr(expr, _) => !is_shatter_runtime_call(expr),
        Stmt::Item(_) => false,
    }
}

fn is_shatter_runtime_call(expr: &Expr) -> bool {
    let Expr::Call(call) = expr else {
        return false;
    };
    let Expr::Path(path) = call.func.as_ref() else {
        return false;
    };
    let mut segments = path
        .path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string());
    matches!(segments.next().as_deref(), Some("shatter_rust_runtime"))
        && matches!(
            segments.next().as_deref(),
            Some("line_hit" | "branch_hit" | "loop_enter")
        )
        && segments.next().is_none()
}

/// Returns true if `expr` is or syntactically contains an `Expr::Let`
/// (i.e. an `if let` head or a let-chain joined by `&&`).
fn contains_let_expr(expr: &Expr) -> bool {
    match expr {
        Expr::Let(_) => true,
        Expr::Binary(bin) => {
            matches!(bin.op, syn::BinOp::And(_))
                && (contains_let_expr(&bin.left) || contains_let_expr(&bin.right))
        }
        Expr::Paren(p) => contains_let_expr(&p.expr),
        _ => false,
    }
}

/// Insert a branch_hit statement at the top of an else branch. The else expression
/// is normally a Block (`else { ... }`) but may be another `if` (`else if ...`).
fn insert_stmt_into_else(else_expr: &mut Expr, stmt: Stmt) {
    match else_expr {
        Expr::Block(block) => {
            block.block.stmts.insert(0, stmt);
        }
        other => {
            // `else if ...` — wrap it in a synthesised block so we can record the
            // false-taken branch hit without disturbing the inner if (which gets
            // instrumented separately on recursion).
            let inner = other.clone();
            let tokens: TokenStream = quote! {
                {
                    #stmt
                    #inner
                }
            };
            if let Ok(new_expr) = syn::parse2::<Expr>(tokens) {
                *other = new_expr;
            }
        }
    }
}

/// Build a JSON constraint string from a condition expression.
fn constraint_for_expr(expr: &Expr) -> String {
    match expr {
        Expr::Binary(bin) => {
            let op = match bin.op {
                syn::BinOp::Eq(_) => "eq",
                syn::BinOp::Ne(_) => "ne",
                syn::BinOp::Lt(_) => "lt",
                syn::BinOp::Le(_) => "le",
                syn::BinOp::Gt(_) => "gt",
                syn::BinOp::Ge(_) => "ge",
                syn::BinOp::And(_) => "and",
                syn::BinOp::Or(_) => "or",
                _ => {
                    return format!(
                        r#"{{"kind":"unknown","hint":"{}"}}"#,
                        escape_json_string(&expr.to_token_stream().to_string())
                    );
                }
            };
            let left = constraint_for_operand(&bin.left);
            let right = constraint_for_operand(&bin.right);
            format!(
                r#"{{"kind":"bin_op","op":"{}","left":{},"right":{}}}"#,
                op, left, right
            )
        }
        Expr::Unary(un) => {
            let op = match un.op {
                syn::UnOp::Not(_) => "not",
                syn::UnOp::Neg(_) => "neg",
                _ => {
                    return format!(
                        r#"{{"kind":"unknown","hint":"{}"}}"#,
                        escape_json_string(&expr.to_token_stream().to_string())
                    );
                }
            };
            let operand = constraint_for_operand(&un.expr);
            format!(r#"{{"kind":"un_op","op":"{}","operand":{}}}"#, op, operand)
        }
        Expr::Path(path) => {
            let name = path.to_token_stream().to_string();
            format!(
                r#"{{"kind":"param","name":"{}","path":[]}}"#,
                escape_json_string(&name)
            )
        }
        Expr::Lit(lit) => constraint_for_lit(lit),
        Expr::MethodCall(mc) => {
            let name = mc.method.to_string();
            format!(
                r#"{{"kind":"unknown","hint":"method call: {}"}}"#,
                escape_json_string(&name)
            )
        }
        Expr::Call(call) => {
            let name = call.func.to_token_stream().to_string();
            format!(
                r#"{{"kind":"unknown","hint":"call: {}"}}"#,
                escape_json_string(&name)
            )
        }
        _ => {
            format!(
                r#"{{"kind":"unknown","hint":"{}"}}"#,
                escape_json_string(&expr.to_token_stream().to_string())
            )
        }
    }
}

/// Build a constraint JSON for a single operand (leaf node).
fn constraint_for_operand(expr: &Expr) -> String {
    match expr {
        Expr::Path(path) => {
            let name = path.to_token_stream().to_string();
            format!(
                r#"{{"kind":"param","name":"{}","path":[]}}"#,
                escape_json_string(&name)
            )
        }
        Expr::Lit(lit) => constraint_for_lit(lit),
        // For compound expressions, recurse
        Expr::Binary(_) | Expr::Unary(_) => constraint_for_expr(expr),
        _ => {
            format!(
                r#"{{"kind":"unknown","hint":"{}"}}"#,
                escape_json_string(&expr.to_token_stream().to_string())
            )
        }
    }
}

/// Build a constraint JSON for a literal expression.
fn constraint_for_lit(lit: &syn::ExprLit) -> String {
    match &lit.lit {
        syn::Lit::Int(i) => {
            format!(
                r#"{{"kind":"const","type":"int","value":{}}}"#,
                i.base10_digits()
            )
        }
        syn::Lit::Float(f) => {
            format!(
                r#"{{"kind":"const","type":"float","value":{}}}"#,
                f.base10_digits()
            )
        }
        syn::Lit::Str(s) => {
            format!(
                r#"{{"kind":"const","type":"str","value":"{}"}}"#,
                escape_json_string(&s.value())
            )
        }
        syn::Lit::Bool(b) => {
            format!(r#"{{"kind":"const","type":"bool","value":{}}}"#, b.value)
        }
        _ => {
            format!(
                r#"{{"kind":"unknown","hint":"literal: {}"}}"#,
                escape_json_string(&lit.to_token_stream().to_string())
            )
        }
    }
}

/// Escape a string for safe inclusion in JSON.
fn escape_json_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: instrument source and return the output.
    fn instrument(source: &str) -> InstrumentResult {
        instrument_source(source, None).expect("instrumentation should succeed")
    }

    /// Helper: instrument a specific function.
    fn instrument_fn(source: &str, func: &str) -> InstrumentResult {
        instrument_source(source, Some(func)).expect("instrumentation should succeed")
    }

    #[test]
    fn simple_if_is_instrumented() {
        let source = r#"
fn check(x: i32) -> bool {
    if x > 0 {
        true
    } else {
        false
    }
}
"#;
        let result = instrument(source);
        assert!(
            result.branch_count >= 1,
            "should have at least 1 branch, got {}",
            result.branch_count
        );
        assert!(
            result.source.contains("shatter_rust_runtime"),
            "instrumented source should reference runtime"
        );
        assert!(
            result.source.contains("branch_hit"),
            "should contain branch_hit call"
        );
    }

    #[test]
    fn function_entry_records_line_hit() {
        let source = r#"
fn check(x: i32) -> i32 {
    x + 1
}
"#;
        let result = instrument_fn(source, "check");
        assert!(
            result.source.contains("line_hit"),
            "instrumented function should record entry line execution"
        );
    }

    #[test]
    fn straight_line_statements_record_line_hits() {
        let source = r#"
fn check(x: i32) -> i32 {
    let doubled = x * 2;
    let shifted = doubled + 1;
    shifted
}
"#;
        let result = instrument_fn(source, "check");
        let line_hits = result.source.matches("line_hit").count();
        assert!(
            line_hits >= 4,
            "instrumented function should record entry and straight-line statements, got {line_hits} hits:\n{}",
            result.source
        );
    }

    #[test]
    fn const_function_entry_does_not_record_line_hit() {
        let source = r#"
const fn answer() -> i32 {
    42
}
"#;
        let result = instrument_fn(source, "answer");
        assert!(
            !result.source.contains("line_hit"),
            "const function entry must not call non-const runtime hooks"
        );
        rustc_check(&result.source).expect("instrumented branchless const fn must compile");
    }

    #[test]
    fn while_loop_is_instrumented() {
        let source = r#"
fn count_up(mut n: i32) -> i32 {
    let mut sum = 0;
    while n > 0 {
        sum += n;
        n -= 1;
    }
    sum
}
"#;
        let result = instrument(source);
        assert!(result.branch_count >= 1);
        assert!(result.source.contains("branch_hit"));
        assert!(result.source.contains("loop_enter"));
    }

    #[test]
    fn while_let_loop_preserves_pattern_binding() {
        let source = r#"
fn first_positive(values: Vec<Option<i32>>) -> i32 {
    let mut iter = values.into_iter();
    while let Some(item) = iter.next() {
        if let Some(value) = item {
            if value > 0 {
                return value;
            }
        }
    }
    0
}
"#;
        let result = instrument(source);
        assert!(result.source.contains("loop_enter"));
        rustc_check(&result.source).expect("instrumented while-let loop must compile");
    }

    #[test]
    fn match_arms_are_instrumented() {
        let source = r#"
fn classify(x: i32) -> &'static str {
    match x {
        0 => "zero",
        1..=9 => "small",
        _ => "large",
    }
}
"#;
        let result = instrument(source);
        // Each match arm should get a branch_hit
        assert!(
            result.branch_count >= 3,
            "expected at least 3 branches for 3 match arms, got {}",
            result.branch_count
        );
        assert!(result.source.contains("branch_hit"));
    }

    #[test]
    fn for_loop_is_instrumented() {
        let source = r#"
fn sum_vec(items: &[i32]) -> i32 {
    let mut total = 0;
    for item in items {
        total += item;
    }
    total
}
"#;
        let result = instrument(source);
        assert!(result.branch_count >= 1);
        assert!(result.source.contains("branch_hit"));
        assert!(result.source.contains("loop_enter"));
    }

    #[test]
    fn nested_branches_get_unique_ids() {
        let source = r#"
fn nested(x: i32, y: i32) -> i32 {
    if x > 0 {
        if y > 0 {
            1
        } else {
            2
        }
    } else {
        3
    }
}
"#;
        let result = instrument(source);
        assert!(
            result.branch_count >= 2,
            "nested branches should get at least 2 IDs, got {}",
            result.branch_count
        );
    }

    #[test]
    fn target_function_only_instruments_named() {
        let source = r#"
fn target(x: i32) -> bool {
    if x > 0 { true } else { false }
}

fn other(y: i32) -> bool {
    if y < 0 { true } else { false }
}
"#;
        let result = instrument_fn(source, "target");
        assert!(
            result.branch_count >= 1,
            "target function should be instrumented"
        );
        // The "other" function should not have been instrumented.
        // We can verify by checking that branch_count is exactly what we expect
        // for just the target function (1 if branch).
        assert_eq!(
            result.branch_count, 1,
            "only the target function's branch should be counted"
        );
    }

    #[test]
    fn no_branches_yields_zero_count() {
        let source = r#"
fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#;
        let result = instrument(source);
        assert_eq!(result.branch_count, 0);
        assert!(!result.source.contains("branch_hit"));
    }

    #[test]
    fn instrumented_source_is_valid_rust_tokens() {
        let source = r#"
fn example(x: i32) -> &'static str {
    if x > 10 {
        "big"
    } else if x > 0 {
        "positive"
    } else {
        "non-positive"
    }
}
"#;
        let result = instrument(source);
        // The output should be parseable as Rust tokens
        let parsed = syn::parse_file(&result.source);
        assert!(
            parsed.is_ok(),
            "instrumented source should be valid Rust: {}",
            parsed.err().map(|e| e.to_string()).unwrap_or_default()
        );
    }

    #[test]
    fn constraint_for_comparison_expr() {
        let expr: Expr = syn::parse_str("x > 0").expect("parse");
        let constraint = constraint_for_expr(&expr);
        assert!(constraint.contains("bin_op"));
        assert!(constraint.contains("gt"));
    }

    #[test]
    fn constraint_for_equality_expr() {
        let expr: Expr = syn::parse_str("x == 42").expect("parse");
        let constraint = constraint_for_expr(&expr);
        assert!(constraint.contains("eq"));
        assert!(constraint.contains("42"));
    }

    #[test]
    fn constraint_for_logical_and() {
        let expr: Expr = syn::parse_str("a && b").expect("parse");
        let constraint = constraint_for_expr(&expr);
        assert!(constraint.contains("and"));
    }

    #[test]
    fn constraint_for_negation() {
        let expr: Expr = syn::parse_str("!flag").expect("parse");
        let constraint = constraint_for_expr(&expr);
        assert!(constraint.contains("un_op"));
        assert!(constraint.contains("not"));
    }

    #[test]
    fn escape_json_handles_special_chars() {
        assert_eq!(escape_json_string(r#"he said "hi""#), r#"he said \"hi\""#);
        assert_eq!(escape_json_string("line\nnewline"), "line\\nnewline");
        assert_eq!(escape_json_string("back\\slash"), "back\\\\slash");
    }

    #[test]
    fn instrument_file_nonexistent_returns_error() {
        let result = instrument_file(Path::new("/nonexistent/file.rs"), None);
        assert!(result.is_err());
        match result.unwrap_err() {
            InstrumentError::FileNotFound(_) => {}
            other => panic!("expected FileNotFound, got {:?}", other),
        }
    }

    #[test]
    fn instrument_source_parse_error() {
        let result = instrument_source("fn broken(", None);
        assert!(result.is_err());
        match result.unwrap_err() {
            InstrumentError::ParseError(_) => {}
            other => panic!("expected ParseError, got {:?}", other),
        }
    }

    #[test]
    fn impl_method_is_instrumented() {
        let source = r#"
struct Foo;

impl Foo {
    fn check(&self, x: i32) -> bool {
        if x > 0 {
            true
        } else {
            false
        }
    }
}
"#;
        let result = instrument(source);
        assert!(result.branch_count >= 1);
        assert!(result.source.contains("branch_hit"));
    }

    /// Compile-check instrumented source via rustc with a stub runtime.
    /// Returns Ok(()) if rustc accepts the source, Err(stderr) otherwise.
    fn rustc_check(instrumented: &str) -> Result<(), String> {
        use std::io::Write;
        let stub = r#"
#[allow(dead_code)]
mod shatter_rust_runtime {
    pub fn branch_hit(_id: u32, _line: u32, _cond: bool, _json: &str) {}
    pub fn line_hit(_line: u32) {}
    pub fn loop_enter(_id: u32) {}
    pub fn mock_call<T>(_name: &str, real: impl FnOnce() -> T) -> T { real() }
}
"#;
        let combined = format!("{stub}\n{instrumented}\n");
        let dir = std::env::temp_dir().join(format!(
            "shatter_iflet_check_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let src_path = dir.join("check.rs");
        let mut f = std::fs::File::create(&src_path).unwrap();
        f.write_all(combined.as_bytes()).unwrap();
        drop(f);
        let out = std::process::Command::new("rustc")
            .arg("--edition=2021")
            .arg("--emit=metadata")
            .arg("--crate-type=lib")
            .arg("-o")
            .arg(dir.join("out.rmeta"))
            .arg(&src_path)
            .arg("-A")
            .arg("warnings")
            .output()
            .expect("rustc should be available");
        let _ = std::fs::remove_dir_all(&dir);
        if out.status.success() {
            Ok(())
        } else {
            Err(format!(
                "rustc rejected instrumented source:\n--- stderr ---\n{}\n--- source ---\n{}",
                String::from_utf8_lossy(&out.stderr),
                combined,
            ))
        }
    }

    #[test]
    fn if_let_some_is_instrumented_and_parses() {
        let source = r#"
fn extract(opt: Option<i32>) -> i32 {
    if let Some(x) = opt {
        x
    } else {
        0
    }
}
"#;
        let result = instrument(source);
        assert!(
            result.branch_count >= 1,
            "if let should produce a branch, got {}",
            result.branch_count
        );
        rustc_check(&result.source).expect("instrumented if let must compile");
    }

    #[test]
    fn if_let_ok_is_instrumented_and_parses() {
        let source = r#"
fn handle(r: Result<String, ()>) -> String {
    if let Ok(s) = r {
        s
    } else {
        String::new()
    }
}
"#;
        let result = instrument(source);
        assert!(result.branch_count >= 1);
        rustc_check(&result.source).expect("instrumented if let Ok must compile");
    }

    #[test]
    fn if_let_in_async_with_await_is_instrumented_and_parses() {
        let source = r#"
async fn fetch<F: std::future::Future<Output = Result<Option<String>, ()>>>(future: F) -> String {
    if let Some(message) = future.await.unwrap_or(None) {
        message
    } else {
        String::from("empty")
    }
}
"#;
        let result = instrument(source);
        assert!(result.branch_count >= 1);
        rustc_check(&result.source).expect("instrumented async if let must compile");
    }

    #[test]
    fn match_with_guard_is_instrumented() {
        let source = r#"
fn guarded(x: i32) -> &'static str {
    match x {
        n if n > 100 => "huge",
        n if n > 0 => "positive",
        _ => "non-positive",
    }
}
"#;
        let result = instrument(source);
        assert!(result.branch_count >= 3);
    }
}
