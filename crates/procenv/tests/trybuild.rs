//! Compile-time tests for EnvConfig derive macro.
//!
//! These tests verify that:
//! - Valid derive usage compiles successfully
//! - Invalid derive usage produces helpful error messages
//!
//! Run with: cargo nextest run --package procenv trybuild

#[test]
fn compile_pass() {
    let t = trybuild::TestCases::new();
    t.pass("tests/compile_pass/*.rs");
}

#[test]
fn compile_fail() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/*.rs");
}
