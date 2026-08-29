//! Huzi code generation library
//!
//! This crate handles LLVM IR code generation from Huzi AST.
//!
//! ## Module Organization
//!
//! - **codegen::mod**: `CodeGen` struct, compile entry point, scope management, public API
//! - **codegen::types**: type registration/layout, LLVM type mapping, coercion utilities
//! - **codegen::stmt**: statement compilation (let, fn, if, for, while, ...)
//! - **codegen::expr**: expression compilation (literals, binary/unary ops, calls, assignment)
//! - **codegen::builtins**: prelude declarations and built-in functions (print, read_*, math, strings)
//! - **codegen::aggregates**: struct literals, enum construction, match, arrays

mod codegen;

pub use codegen::CodeGen;
