//! Compile-fail and compile-pass tests for the `view!` and `#[component]` macros.
//!
//! `pass_*` files must compile. `fail_*` files must be rejected with the stderr
//! recorded next to them (`*.stderr`). Regenerate the snapshots after an
//! intentional message change with `TRYBUILD=overwrite cargo test -p tpt-appfront-core --test trybuild`,
//! then review the diff. rustc's wording can change between releases, so a
//! snapshot mismatch after a toolchain upgrade is expected to need a refresh.

#[test]
fn ui() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/pass_*.rs");
    cases.compile_fail("tests/ui/fail_*.rs");
}
