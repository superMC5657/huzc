mod cli;
mod linker;
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

    let source = read_source(&args.input);
    let program = parse_source(source);

    // [3/5] Compiling
    println!("[3/5] Compiling...");
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "huzi");
    if let Err(e) = codegen.compile(&program) {
        die(format!("Compile error: {}", e));
    }

    // [4/5] Verifying
    println!("[4/5] Verifying...");
    let paths = OutputPaths::new(&args.output);
    write_ir(&codegen, &paths.ll_path);
    if !codegen.verify() {
        die("Error: LLVM module verification failed (this is a compiler bug)".to_string());
    }

    // [5/5] Generating executable
    println!("[5/5] Generating executable...");
    compile_ir_to_object(&paths);
    println!("  Linking to executable...");
    link(&paths, args.linker);

    // Cleanup intermediate files
    let _ = fs::remove_file(&paths.ll_path);
    let _ = fs::remove_file(&paths.obj_path);

    println!("✓ {} generated successfully!", paths.exe_path.display());
}

/// Read the Huzi source file to compile.
fn read_source(input: &str) -> String {
    fs::read_to_string(input).unwrap_or_else(|e| die(format!("Error reading file: {}", e)))
}

/// [1/5] Lexing + [2/5] Parsing: turn source text into the program AST.
fn parse_source(source: String) -> Program {
    println!("[1/5] Lexing...");
    let tokens = Lexer::new(source)
        .tokenize()
        .unwrap_or_else(|e| die(format!("Lex error: {}", e)));

    println!("[2/5] Parsing...");
    HuziParser::new(tokens)
        .parse()
        .unwrap_or_else(|e| die(format!("Parse error: {}", e)))
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
