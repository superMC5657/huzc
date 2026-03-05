use clap::Parser;
use huzi_codegen::CodeGen;
use huzi_lexer::Lexer;
use huzi_parser::Parser as HuziParser;
use inkwell::context::Context;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Huzi Programming Language Compiler
#[derive(Parser, Debug)]
#[command(name = "huzc")]
#[command(about = "Compile Huzi source code to executable")]
struct Args {
    /// Input source file (.hz)
    #[arg(short, long)]
    input: String,

    /// Output file name (without extension)
    #[arg(short, long, default_value = "a")]
    output: String,
}

/// Get platform-specific executable extension
fn get_exe_ext() -> &'static str {
    if cfg!(target_os = "windows") {
        "exe"
    } else {
        ""
    }
}

/// Get platform-specific object file extension
fn get_obj_ext() -> &'static str {
    if cfg!(target_os = "windows") {
        "obj"
    } else {
        "o"
    }
}

/// Build output path with platform-specific extension
fn build_output_path(output: &str) -> PathBuf {
    if output.ends_with(".exe") || output.ends_with(".o") || output.ends_with(".obj") {
        PathBuf::from(output)
    } else {
        let ext = get_exe_ext();
        if ext.is_empty() {
            PathBuf::from(output)
        } else {
            PathBuf::from(format!("{}.{}", output, ext))
        }
    }
}

/// Get intermediate file path (same directory as output)
fn build_intermediate_path(output: &str, ext: &str) -> PathBuf {
    let output_path = PathBuf::from(output);
    let output_dir = output_path.parent().unwrap_or(Path::new(""));
    let stem = output_path.file_stem().unwrap().to_str().unwrap();
    
    if output_dir.as_os_str().is_empty() {
        PathBuf::from(format!("{}.{}", stem, ext))
    } else {
        output_dir.join(format!("{}.{}", stem, ext))
    }
}

/// Run a command and handle errors
fn run_command(cmd: &str, args: &[&str]) -> Result<(), String> {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run {}: {}", cmd, e))?;

    if !output.status.success() {
        return Err(format!("{} error:\n{}", cmd, String::from_utf8_lossy(&output.stderr)));
    }
    Ok(())
}

fn main() {
    let args = Args::parse();

    // Read source file
    let source = fs::read_to_string(&args.input)
        .unwrap_or_else(|e| {
            eprintln!("Error reading file: {}", e);
            std::process::exit(1);
        });

    // [1/5] Lexing
    println!("[1/5] Lexing...");
    let tokens = Lexer::new(source).tokenize().unwrap_or_else(|e| {
        eprintln!("Lex error: {}", e);
        std::process::exit(1);
    });

    // [2/5] Parsing
    println!("[2/5] Parsing...");
    let program = HuziParser::new(tokens).parse().unwrap_or_else(|e| {
        eprintln!("Parse error: {}", e);
        std::process::exit(1);
    });

    // [3/5] Compiling
    println!("[3/5] Compiling...");
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "huzi");

    if let Err(e) = codegen.compile(&program) {
        eprintln!("Compile error: {}", e);
        std::process::exit(1);
    }

    // [4/5] Verifying
    println!("[4/5] Verifying...");
    if !codegen.verify() {
        eprintln!("Warning: Verification failed, but continuing...");
    }

    // [5/5] Generating executable
    println!("[5/5] Generating executable...");

    // Build paths
    let exe_path = build_output_path(&args.output);
    let ll_path = build_intermediate_path(&args.output, "ll");
    let obj_path = build_intermediate_path(&args.output, get_obj_ext());

    // Write LLVM IR
    if let Err(e) = codegen.write_ir_to_file(ll_path.to_str().unwrap()) {
        eprintln!("Error writing IR: {}", e);
        std::process::exit(1);
    }

    // Compile IR to object file
    println!("  Compiling LLVM IR to object file...");
    run_command("llc", &[
        "--relocation-model=pic",
        "--filetype=obj",
        "-o", obj_path.to_str().unwrap(),
        ll_path.to_str().unwrap(),
    ]).unwrap_or_else(|e| {
        eprintln!("{}", e);
        std::process::exit(1);
    });

    // Link to executable
    println!("  Linking to executable...");
    
    // Try lld-link first (Windows)
    let lld_args = [
        format!("/OUT:{}", exe_path.to_str().unwrap()),
        "/ENTRY:main".to_string(),
        "/LIBPATH:C:\\Program Files (x86)\\Windows Kits\\10\\lib\\10.0.26100.0\\ucrt\\x64".to_string(),
        "/DEFAULTLIB:ucrt.lib".to_string(),
        "/DEFAULTLIB:msvcrt.lib".to_string(),
        obj_path.to_str().unwrap().to_string(),
    ];
    let lld_args_ref: Vec<&str> = lld_args.iter().map(|s| s.as_str()).collect();
    
    let use_clang = run_command("lld-link", &lld_args_ref).is_err();
    
    // Fall back to clang
    if use_clang {
        println!("  Trying clang...");
        let clang_target = if cfg!(target_os = "windows") {
            "x86_64-pc-windows-msvc"
        } else if cfg!(target_os = "macos") {
            "x86_64-apple-darwin"
        } else {
            "x86_64-unknown-linux-gnu"
        };
        
        run_command("clang", &[
            "-o", exe_path.to_str().unwrap(),
            "-target", clang_target,
            obj_path.to_str().unwrap(),
        ]).unwrap_or_else(|e| {
            eprintln!("{}", e);
            std::process::exit(1);
        });
    }

    // Cleanup intermediate files
    // let _ = fs::remove_file(&ll_path);
    // let _ = fs::remove_file(&obj_path);

    println!("✓ {} generated successfully!", exe_path.display());
}
