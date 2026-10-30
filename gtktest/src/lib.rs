//! `#[gtktest::test]` — marks a GTK test body so it can run under **either** harness.
//!
//! # Why this exists
//!
//! `#[gtk::test]` dispatches its body onto a `glib::ThreadPool` worker
//! (`gtk4/src/lib.rs`, `test_synced`) and calls `gtk::init()` there. That is fine
//! where GTK will initialise on any thread, and impossible where GTK reserves the
//! process main thread for windowing — the body aborts before it runs, and
//! `--test-threads=1` cannot help because libtest spawns a thread per test
//! regardless. So the whole GTK suite is unavailable on such a platform.
//!
//! The fix is a runner that owns `main()`. This attribute is what lets one body
//! serve both runners at once, so the existing libtest run is not traded away for
//! the portable one.
//!
//! # What it emits
//!
//! For `#[gtktest::test] fn foo() { BODY }`:
//!
//! ```ignore
//! fn __gtktest_body_foo() { BODY }                     // 1. the body, renamed
//! #[gtk::test] fn foo() {                               // 2. the libtest path
//!     crate::gtk_log_harness::install_once();           //    TDD 21.13 — see below
//!     crate::focusrelease::install();                   //    `src/focusrelease.rs`
//!     __gtktest_body_foo()
//! }
//! inventory::submit! { Case { name: "…::foo", run: __gtktest_body_foo } }  // 3. runner
//! ```
//!
//! The `install_once()` call in item 2 arms the libtest harness's own collapsing
//! glib log writer, exactly once per process — `#[gtk::test]` initialises GTK on
//! a worker thread this crate has no other hook into, so the generated wrapper
//! every libtest GTK body passes through is the one shared init point available.
//! It is a no-op after the first call (`gtk_log_harness::install_once`'s own
//! `OnceLock`), and it is inert in the suite binary along with the rest of item
//! 2 (stripped where `--test` is absent) — `src/gtk_suite.rs`'s `main()` calls
//! the same function itself, directly, once, for that harness.
//!
//! Item 1 is the annotated item's **own TokenStream with exactly one token replaced**
//! — the name. The body's `Group` passes through untouched, so a diagnostic inside a
//! body still points at the body as written.
//!
//! **The visible name stays on item 2, deliberately.** The libtest test keeps the
//! function's original name, so `cargo test <name>` invocations, the full test paths
//! cited in `sdd/ISSUES.md`, and anything else that greps a test name all keep
//! working. The mangled name is the one nobody types.
//!
//! Item 2 is why the split works, and it rests on a measured rustc behaviour: cargo
//! builds a `harness = false` target with `--cfg test` but **not** `--test`, and
//! rustc *removes* `#[test]` items when `--test` is absent. So in the suite's crate
//! item 2 vanishes entirely (`gtk::test_synced` is never even referenced), while in
//! the ordinary lib test target it is the normal `#[gtk::test]` test it always was.
//! Item 3 survives in both; only the suite iterates the registry.
//!
//! The consequence worth stating: **one attribute, two harnesses, no duplicated
//! body.** A body cannot drift between the two runs, because there is only one.

use proc_macro::{TokenStream, TokenTree};

/// Register a GTK test body with both the libtest harness and the main-thread suite.
///
/// Apply exactly as `#[gtk::test]` was applied — it takes no arguments and the body
/// needs no changes:
///
/// ```ignore
/// #[gtktest::test]
/// fn the_window_opens_with_one_tab() { /* … */ }
/// ```
#[proc_macro_attribute]
pub fn test(attr: TokenStream, item: TokenStream) -> TokenStream {
    assert!(
        attr.is_empty(),
        "#[gtktest::test] takes no arguments (got `{attr}`). It is a drop-in for the \
         bare `#[gtk::test]`; if you need an argument-taking attribute, the suite \
         runner has to learn about it first."
    );

    // Harness attributes must ride item 2, not item 1. See `split_harness_attrs`.
    let (item, harness_attrs) = split_harness_attrs(item);
    // Same head-token classification `split_harness_attrs` used to route these here in
    // the first place — `#[ignore = "flaky"]` must count as ignored exactly as bare
    // `#[ignore]` does, not just the routing decision but this boolean too.
    let ignored = harness_attrs
        .iter()
        .any(|a| harness_attr_head(a) == "ignore");
    let should_panic = harness_attrs
        .iter()
        .any(|a| harness_attr_head(a) == "should_panic");
    // `should_panic(expected = "…")` is REFUSED, not accepted and reduced to the bare
    // form. The suite runner (`src/gtk_suite.rs`) carries only a boolean, so it would count
    // ANY panic as the expected one — an unrelated early `unwrap` would pass a guard
    // written to fail for one stated reason. libtest checks the payload; a custom runner
    // that adopted the grammar without the check would be honouring half of it silently.
    // Teach `Case` and `run_case` to match the payload before lifting this.
    if let Some(a) = harness_attrs
        .iter()
        .find(|a| harness_attr_head(a) == "should_panic" && a.trim() != "should_panic")
    {
        panic!(
            "#[gtktest::test] supports only bare `#[should_panic]`, not `#[{a}]`: the GTK \
             suite runner cannot check a panic's message, so `expected = …` would pass on \
             any panic. Use bare `#[should_panic]` and assert the message inside the body, \
             or teach src/gtk_suite.rs's `Case`/`run_case` to match it first."
        );
    }

    let (renamed_item, name) = rename_fn(item).unwrap_or_else(|| {
        panic!(
            "#[gtktest::test] must be applied to a free function (`fn <name>() {{ … }}`); \
             no `fn <name>` was found in the annotated item"
        )
    });
    let body = format!("__gtktest_body_{name}");
    let harness_attr_text: String = harness_attrs
        .iter()
        .map(|a| format!("#[{a}]\n"))
        .collect::<Vec<_>>()
        .join("");

    // Generated by string, then parsed — deliberately, and only for the two items
    // that contain no user code. `run: {body}` coerces the function to the `fn()`
    // pointer the registry holds; `module_path!()` expands at *this* call site, so
    // the recorded name is the body's real module path.
    //
    // `crate::gtk_log_harness::install_once()` is the libtest harness's one-time
    // init point for the collapsing glib log writer (TDD 21.13): `#[gtk::test]`
    // calls `gtk::init()` inside gtk4-rs's own generated wrapper, on a worker
    // thread this crate cannot hook — but every libtest-run GTK body passes
    // through exactly this generated function, so installing here, guarded by
    // `install_once`'s own `OnceLock`, reaches the writer into place before the
    // FIRST such body runs, whichever body that turns out to be, at the cost of
    // one atomic load per test thereafter. This line is inert in the suite
    // binary along with the rest of this item — `#[test]` items (and everything
    // they alone reference) are stripped where `--test` is absent, and the
    // suite's `main()` calls the same function itself, directly, once.
    let generated = format!(
        "{harness_attr_text}\
         #[gtk::test]\n\
         fn {name}() {{ crate::gtk_log_harness::install_once(); crate::focusrelease::install(); {body}() }}\n\
         inventory::submit! {{\n\
         \x20   crate::suite_registry::Case {{\n\
         \x20       name: concat!(module_path!(), \"::\", \"{name}\"),\n\
         \x20       run: {body},\n\
         \x20       ignored: {ignored},\n\
         \x20       should_panic: {should_panic},\n\
         \x20   }}\n\
         }}\n"
    );

    let mut out = renamed_item;
    out.extend(
        generated
            .parse::<TokenStream>()
            .expect("generated registration is valid Rust"),
    );
    out
}

/// The classifying head token of a bracketed attribute's inner text, e.g. `"ignore"`
/// for both `ignore` and `ignore = "flaky"`, or `"should_panic"` for both
/// `should_panic` and `should_panic(expected = "…")`.
///
/// The one place this decision is made. `split_harness_attrs` uses it to route an
/// attribute; the `ignored` boolean below uses it again on the routed result — same
/// question, same answer, rather than two rules that can silently disagree on
/// `#[ignore = "…"]`.
fn harness_attr_head(inner: &str) -> &str {
    inner
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .next()
        .unwrap_or_default()
}

/// Split the harness attributes (`#[ignore]`, `#[should_panic…]`) off the front of
/// the annotated item, returning the remaining item and the attributes' inner text.
///
/// # Why this is not cosmetic
///
/// Every token of the annotated item used to be moved wholesale onto item 1 — the
/// renamed body, which is a **plain function**. `#[ignore]` on a plain function is
/// inert: it compiles, warns about nothing useful, and does nothing. Item 2 — the
/// actual `#[gtk::test]` — got no `#[ignore]` at all, so a body the author had
/// explicitly marked ignored **ran anyway**, under both harnesses.
///
/// That is the worst shape a test-infrastructure bug can take: the author's
/// instruction is silently discarded, and the failure mode is a test running when it
/// was meant to be quarantined — typically a known-flaky one, so it presents as
/// intermittent suite failure with no apparent cause. Nothing in the tree used
/// `#[ignore]` when this was found, which is exactly why it survived: the first
/// author to reach for it would have paid for it.
///
/// `#[should_panic]` moves for the same reason — libtest, not the body, interprets it.
/// Everything else (doc comments, `#[cfg]`, `#[allow]`) deliberately stays on the
/// body, where it governs the code the author wrote.
fn split_harness_attrs(item: TokenStream) -> (TokenStream, Vec<String>) {
    let tokens: Vec<TokenTree> = item.into_iter().collect();
    let mut kept = Vec::new();
    let mut harness = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        // An outer attribute is exactly `Punct('#')` followed by a bracketed `Group`.
        if let (TokenTree::Punct(punct), Some(TokenTree::Group(group))) =
            (&tokens[i], tokens.get(i + 1))
        {
            if punct.as_char() == '#' && group.delimiter() == proc_macro::Delimiter::Bracket {
                let inner = group.stream().to_string();
                let head = harness_attr_head(&inner);
                if head == "ignore" || head == "should_panic" {
                    harness.push(inner);
                    i += 2;
                    continue;
                }
            }
        }
        kept.push(tokens[i].clone());
        i += 1;
    }
    (kept.into_iter().collect(), harness)
}

/// Rename the function to `__gtktest_body_<name>`, returning the rewritten item and
/// the original name.
///
/// Exactly one token changes: the identifier immediately after the first top-level
/// `fn`. Everything else — attributes, doc comments, and above all the body `Group` —
/// is moved through unchanged, which is what keeps diagnostics inside a body pointing
/// at the body. Deliberately not a `syn` signature parse: locating one identifier
/// does not need one, and this keeps `syn` out of the workspace.
fn rename_fn(item: TokenStream) -> Option<(TokenStream, String)> {
    let mut out = Vec::new();
    let mut name = None;
    let mut saw_fn = false;
    for token in item {
        match &token {
            TokenTree::Ident(ident) if saw_fn && name.is_none() => {
                let original = ident.to_string();
                let mut renamed =
                    proc_macro::Ident::new(&format!("__gtktest_body_{original}"), ident.span());
                renamed.set_span(ident.span());
                out.push(TokenTree::Ident(renamed));
                name = Some(original);
                saw_fn = false;
                continue;
            }
            TokenTree::Ident(ident) => saw_fn = ident.to_string() == "fn",
            // Attributes and doc comments arrive as Punct + Group; neither can
            // contain the top-level `fn`, so resetting on anything else is safe.
            _ => saw_fn = false,
        }
        out.push(token);
    }
    Some((out.into_iter().collect(), name?))
}
