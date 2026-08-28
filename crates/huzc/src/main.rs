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
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!("{} error:\nstdout: {}\nstderr: {}", cmd, stdout, stderr));
    }
    Ok(())
}

/// Locate the Windows SDK ucrt/um lib directories. Uses the environment set by
/// a VS developer prompt when available; otherwise falls back to a known path.
fn get_sdk_libpaths() -> (Option<String>, Option<String>) {
    let sdk_dir = std::env::var("WindowsSdkDir").ok();
    let sdk_ver = std::env::var("WindowsSDKVersion")
        .ok()
        .map(|v| v.trim_end_matches('\\').to_string());

    if let (Some(dir), Some(ver)) = (&sdk_dir, &sdk_ver) {
        let base = Path::new(dir).join("lib").join(ver);
        return (
            Some(base.join("ucrt").join("x64").to_string_lossy().into_owned()),
            Some(base.join("um").join("x64").to_string_lossy().into_owned()),
        );
    }

    // Fallback: newest installed Windows Kits version.
    let kits = Path::new("C:\\Program Files (x86)\\Windows Kits\\10\\lib");
    if let Ok(entries) = fs::read_dir(kits) {
        let mut versions: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("10."))
            .collect();
        versions.sort();
        if let Some(ver) = versions.pop() {
            let base = kits.join(ver);
            return (
                Some(base.join("ucrt").join("x64").to_string_lossy().into_owned()),
                Some(base.join("um").join("x64").to_string_lossy().into_owned()),
            );
        }
    }

    (None, None)
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

    // Build paths
    let exe_path = build_output_path(&args.output);
    let ll_path = build_intermediate_path(&args.output, "ll");
    let obj_path = build_intermediate_path(&args.output, get_obj_ext());

    // Write LLVM IR before verifying so it can be inspected on failure.
    if let Err(e) = codegen.write_ir_to_file(ll_path.to_str().unwrap()) {
        eprintln!("Error writing IR: {}", e);
        std::process::exit(1);
    }

    if !codegen.verify() {
        eprintln!("Error: LLVM module verification failed (this is a compiler bug)");
        std::process::exit(1);
    }

    // [5/5] Generating executable
    println!("[5/5] Generating executable...");

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
    let (ucrt_path, um_path) = get_sdk_libpaths();
    let mut lld_args: Vec<String> = vec![
        format!("/OUT:{}", exe_path.to_str().unwrap()),
        "/ENTRY:main".to_string(),
    ];
    if let Some(p) = &ucrt_path {
        lld_args.push(format!("/LIBPATH:{}", p));
    }
    if let Some(p) = &um_path {
        lld_args.push(format!("/LIBPATH:{}", p));
    }
    lld_args.extend([
        "/DEFAULTLIB:ucrt.lib".to_string(),
        "/DEFAULTLIB:msvcrt.lib".to_string(),
        "/DEFAULTLIB:legacy_stdio_definitions.lib".to_string(),
        obj_path.to_str().unwrap().to_string(),
    ]);
    let lld_args_ref: Vec<&str> = lld_args.iter().map(|s| s.as_str()).collect();

    let lld_success = run_command("lld-link", &lld_args_ref).is_ok();

    // Fall back to clang
    if !lld_success {
        println!("  Trying clang...");
        let clang_target = if cfg!(target_os = "windows") {
            "x86_64-pc-windows-msvc"
        } else if cfg!(target_os = "macos") {
            "x86_64-apple-darwin"
        } else {
            "x86_64-unknown-linux-gnu"
        };

        // Add libraries for C standard functions (printf, malloc, sprintf, etc.)
        let mut clang_args = vec![
            "-o".to_string(), exe_path.to_str().unwrap().to_string(),
            "-target".to_string(), clang_target.to_string(),
        ];
        
        if cfg!(target_os = "windows") {
            clang_args.extend(vec![
                obj_path.to_str().unwrap().to_string(),
                "-lucrt".to_string(),
                "-llegacy_stdio_definitions".to_string(),
            ]);
        } else {
            clang_args.insert(2, obj_path.to_str().unwrap().to_string());
        }
        
        let clang_args_ref: Vec<&str> = clang_args.iter().map(|s| s.as_str()).collect();
        run_command("clang", &clang_args_ref).unwrap_or_else(|e| {
            eprintln!("{}", e);
            std::process::exit(1);
        });
    }

    // Cleanup intermediate files
    // let _ = fs::remove_file(&ll_path);
    // let _ = fs::remove_file(&obj_path);

    println!("✓ {} generated successfully!", exe_path.display());
}
