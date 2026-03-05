//! Huzi code generation library
//!
//! This crate handles LLVM IR code generation from Huzi AST.
//! 
//! ## Module Organization
//! 
//! The code is organized in a single file for simplicity, but divided into clear sections:
//! 
//! - **CodeGen struct** (lines 1-100): Main code generator context
//! - **Prelude** (lines 100-200): Standard library function declarations
//! - **Statement compilation** (lines 200-500): let, fn, if, for, while, etc.
//! - **Expression compilation** (lines 500-900): literals, binary/unary ops, calls
//! - **Standard library** (lines 900-1400): print, read_*, math functions
//! - **Utilities** (lines 1400+): type conversion, helpers

mod codegen;

pub use codegen::CodeGen;
