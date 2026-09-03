//! The checks, and the one place their order and their reporting live.
//!
//! Each check prints its number and title as it executes, then PASS or the findings. That
//! enumeration IS the gate's inventory — nothing counts the checks, here or in POLICY,
//! because a count is the one fact about a growing list guaranteed to be wrong by the next
//! addition and no reader can tell a stale one from a current one.

pub mod architecture;
pub mod manpage;
pub mod references;
pub mod register;

use crate::lint::Tree;

/// Run every check over `tree`, in number order. Returns true when the tree passes.
///
/// Every check runs even after one has failed: the output is read as a worklist, and a
/// gate that stops at the first finding turns one pass into N.
pub fn run_all(tree: &Tree) -> bool {
    let verdicts = [
        references::issues_cited_outside_register(tree),
        references::scrap_cited_in_src_exists(tree),
        references::scrap_cited_in_register_exists(tree),
        architecture::module_list_drift(tree),
        architecture::legacy_gtk_test_attribute(tree),
        architecture::legacy_attribute_prescribed(tree),
        references::document_paths_resolve(tree),
        references::app_id_drift(tree),
        references::bare_ap_citations(tree),
        register::number_immutability(tree),
        register::stub_keeps_implementation_line(tree),
        register::growth(tree),
        architecture::windows_illegal_paths(tree),
        register::body_without_toc_row(tree),
        architecture::powershell_encoding(tree),
        architecture::parser_dispatch_exhaustive(tree),
        manpage::documented_vocabulary(tree),
        manpage::about_dialog_parity(tree),
        register::duplicate_entry_numbers(tree),
    ];
    verdicts.into_iter().all(|ok| ok)
}

/// `── Check <id>: <title> ──`
pub fn header(id: &str, title: &str) {
    println!("── Check {id}: {title} ──");
}

pub fn pass() -> bool {
    println!("  PASS");
    true
}

/// Print a finding: a headline, the offending lines, then the advice that says what to do
/// about it. Returns false, so a check body reads `return fail(...)`.
pub fn fail(headline: &str, findings: &[String], advice: &[&str]) -> bool {
    println!("  FAIL — {headline}");
    for finding in findings {
        println!("    {finding}");
    }
    for line in advice {
        println!("    {line}");
    }
    false
}
