mod cli;
mod linker;
mod modules;
mod paths;

use clap::Parser;
use cli::Args;
use huzi_ast::Program;
use huzi_codegen::CodeGen;
use huzi_lexer::Lexer;
use huzi_parser::Parser as HuziParser;
use inkwell::context::Context;
use linker::{link, run_command};
use paths::OutputPaths;
use std::fs;
use std::path::Path;

/// Print an error to stderr and exit with status 1.
pub(crate) fn die(msg: String) -> ! {
    eprintln!("{}", msg);
    std::process::exit(1);
}

fn main() {
    let args = Args::parse();
    // Release mode runs silently: no progress logs, errors still go to stderr.
    let quiet = args.release;

    let source = read_source(&args.input);
    let mut program = parse_source(&source, quiet);

    // 解析 import:内置模块直接注册,文件模块递归加载(去重 + 防循环)。
    let base_dir = Path::new(&args.input)
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();
    let imported = modules::load_modules(&mut program, &base_dir);

    // [3/5] Compiling
    if !quiet {
        println!("[3/5] Compiling...");
    }
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "huzi");
    for (name, module_program) in &imported {
        codegen.add_module(name, module_program.as_ref());
    }
    if let Err(e) = codegen.compile(&program) {
        die(format!("Compile error: {}", e));
    }

    // [4/5] Verifying
    if !quiet {
        println!("[4/5] Verifying...");
    }
    let paths = OutputPaths::new(&args.output);
    write_ir(&codegen, &paths.ll_path);
    if !codegen.verify() {
        die("Error: LLVM module verification failed (this is a compiler bug)".to_string());
    }

    // Optimization: run the LLVM IR optimizer before code generation when the
    // effective level is above 0 (`--release` maps to level 2). Level 0 (dev
    // mode) passes the raw inkwell IR straight to llc.
    let opt_level = args.effective_opt_level();
    if opt_level > 0 {
        optimize_ir(&paths, opt_level, quiet);
    }

    // [5/5] Generating executable
    if !quiet {
        println!("[5/5] Generating executable...");
    }
    compile_ir_to_object(&paths);
    if !quiet {
        println!("  Linking to executable...");
    }
    link(&paths, args.linker, quiet);

    // Cleanup intermediate files
    let _ = fs::remove_file(&paths.ll_path);
    let _ = fs::remove_file(&paths.obj_path);

    if !quiet {
        println!("✓ {} generated successfully!", paths.exe_path.display());
    }
}

/// Read the Huzi source file to compile.
fn read_source(input: &str) -> String {
    fs::read_to_string(input).unwrap_or_else(|e| die(format!("Error reading file: {}", e)))
}

/// [1/5] Lexing + [2/5] Parsing: turn source text into the program AST.
/// Lex/parse errors carry real line/column positions, so they are rendered
/// with a source excerpt via huzi-error.
fn parse_source(source: &str, quiet: bool) -> Program {
    if !quiet {
        println!("[1/5] Lexing...");
    }
    let tokens = Lexer::new(source.to_string())
        .tokenize()
        .unwrap_or_else(|e| die(huzi_error::render(&e, source, "Lex error")));

    if !quiet {
        println!("[2/5] Parsing...");
    }
    HuziParser::new(tokens)
        .parse()
        .unwrap_or_else(|e| die(huzi_error::render(&e, source, "Parse error")))
}

/// Write LLVM IR to disk before verifying so it can be inspected on failure.
fn write_ir(codegen: &CodeGen, ll_path: &Path) {
    if let Err(e) = codegen.write_ir_to_file(ll_path.to_str().unwrap()) {
        die(format!("Error writing IR: {}", e));
    }
}

/// Compile the LLVM IR to a platform object file with llc.
fn compile_ir_to_object(paths: &OutputPaths) {
    run_command("llc", &[
        "--relocation-model=pic",
        "--filetype=obj",
        "-o", paths.obj_path.to_str().unwrap(),
        paths.ll_path.to_str().unwrap(),
    ])
    .unwrap_or_else(|e| die(e));
}

/// Optimize the LLVM IR in place with `opt -O<level>` (only called when
/// level > 0). `opt` ships with LLVM alongside `llc`, so no extra toolchain
/// is needed. The level's pass pipeline covers inlining, constant folding
/// and common-subexpression elimination.
fn optimize_ir(paths: &OutputPaths, level: u8, quiet: bool) {
    let ll_path = paths.ll_path.to_str().unwrap().to_string();
    let opt_args: Vec<String> = vec![
        "-S".to_string(),
        format!("-O{}", level),
        "-o".to_string(),
        ll_path.clone(),
        ll_path,
    ];
    let opt_args_ref: Vec<&str> = opt_args.iter().map(|s| s.as_str()).collect();
    run_command("opt", &opt_args_ref).unwrap_or_else(|e| die(e));
    if !quiet {
        println!("  [opt] -O{} optimization applied", level);
    }
}
