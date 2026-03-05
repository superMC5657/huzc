use clap::Parser;
use huzi_codegen::CodeGen;
use huzi_lexer::Lexer;
use huzi_parser::Parser as HuziParser;
use inkwell::context::Context;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Parser, Debug)]
#[command(name = "huzc")]
#[command(about = "Huzi Programming Language Compiler")]
struct Args {
    #[arg(short, long)]
    input: String,

    #[arg(short, long, default_value = "a")]
    output: String,

    #[arg(long, default_value = "false")]
    emit_llvm: bool,

    #[arg(long, default_value = "false")]
    only_compile: bool,
}

fn get_platform_exe_ext() -> &'static str {
    if cfg!(target_os = "windows") {
        "exe"
    } else if cfg!(target_os = "macos") {
        ""
    } else {
        ""
    }
}

fn get_platform_obj_ext() -> &'static str {
    if cfg!(target_os = "windows") {
        "obj"
    } else {
        "o"
    }
}

fn main() {
    let args = Args::parse();

    let source = match fs::read_to_string(&args.input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading file: {}", e);
            std::process::exit(1);
        }
    };

    println!("[1/5] Lexing...");
    let mut lexer = Lexer::new(source);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Lex error: {}", e);
            std::process::exit(1);
        }
    };

    println!("[2/5] Parsing...");
    let mut parser = HuziParser::new(tokens);
    let program = match parser.parse() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Parse error: {}", e);
            std::process::exit(1);
        }
    };

    println!("[3/5] Compiling...");
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, "huzi");

    if let Err(e) = codegen.compile(&program) {
        eprintln!("Compile error: {}", e);
        std::process::exit(1);
    }

    println!("[4/5] Verifying...");
    let verify_result = codegen.verify();
    if !verify_result {
        eprintln!("Warning: Verification failed, but continuing...");
        eprintln!("Generated IR:");
        eprintln!("{}", codegen.print_llvm_ir());
    }

    // Determine output paths
    let output_path = PathBuf::from(&args.output);
    let output_dir = output_path.parent().unwrap_or(Path::new(""));
    
    // Get the base name for intermediate files from output
    let output_stem = output_path.file_stem().unwrap().to_str().unwrap();
    
    // Build output executable path with platform-specific extension
    let exe_path = if args.output.ends_with(".exe") || args.output.ends_with(".o") || args.output.ends_with(".obj") {
        PathBuf::from(&args.output)
    } else {
        let exe_ext = get_platform_exe_ext();
        if exe_ext.is_empty() {
            PathBuf::from(&args.output)
        } else {
            PathBuf::from(format!("{}.{}", args.output, exe_ext))
        }
    };
    
    // Intermediate files go to the same directory as output, with same base name
    let ll_path = if output_dir.as_os_str().is_empty() {
        PathBuf::from(format!("{}.ll", output_stem))
    } else {
        output_dir.join(format!("{}.ll", output_stem))
    };
    
    let obj_path = if output_dir.as_os_str().is_empty() {
        PathBuf::from(format!("{}.{}", output_stem, get_platform_obj_ext()))
    } else {
        output_dir.join(format!("{}.{}", output_stem, get_platform_obj_ext()))
    };

    if args.emit_llvm {
        println!("[5/5] Writing LLVM IR...");
        let llvm_output_path = if args.output.ends_with(".ll") {
            PathBuf::from(&args.output)
        } else {
            PathBuf::from(format!("{}.ll", args.output))
        };
        if let Err(e) = codegen.write_ir_to_file(&llvm_output_path.to_str().unwrap()) {
            eprintln!("Error writing IR: {}", e);
            std::process::exit(1);
        }
        println!("✓ LLVM IR written to {}", llvm_output_path.display());
        return;
    }

    println!("[5/5] Generating executable...");

    let ll_path_str = ll_path.to_str().unwrap();
    if let Err(e) = codegen.write_ir_to_file(ll_path_str) {
        eprintln!("Error writing IR: {}", e);
        std::process::exit(1);
    }

    println!("  Compiling LLVM IR to object file...");
    let llc_output = Command::new("llc")
        .args([
            "--relocation-model=pic",
            "--filetype=obj",
            "-o",
            obj_path.to_str().unwrap(),
            ll_path_str,
        ])
        .output();

    match llc_output {
        Ok(output) => {
            if !output.status.success() {
                eprintln!("llc error:\n{}", String::from_utf8_lossy(&output.stderr));
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("Failed to run llc: {}", e);
            std::process::exit(1);
        }
    }

    if args.only_compile {
        if let Err(e) = fs::copy(&obj_path, &exe_path) {
            eprintln!("Error copying object file: {}", e);
            std::process::exit(1);
        }
        println!("✓ Object file written to {}", exe_path.display());

        let _ = fs::remove_file(&ll_path);
        let _ = fs::remove_file(&obj_path);
        return;
    }

    println!("  Linking to executable...");

    // Try lld-link first, then fall back to clang
    let lld_output = Command::new("lld-link")
        .args([
            &format!("/OUT:{}", exe_path.to_str().unwrap()),
            "/ENTRY:main",
            "/LIBPATH:C:\\Program Files (x86)\\Windows Kits\\10\\lib\\10.0.26100.0\\ucrt\\x64",
            "/DEFAULTLIB:ucrt.lib",
            "/DEFAULTLIB:msvcrt.lib",
            obj_path.to_str().unwrap(),
        ])
        .output();

    let success = match lld_output {
        Ok(output) => output.status.success(),
        Err(_) => false,
    };

    if !success {
        // Fall back to clang
        println!("  Trying clang...");
        let clang_output = Command::new("clang")
            .args([
                "-o",
                exe_path.to_str().unwrap(),
                "-target",
                "x86_64-pc-windows-msvc",
                obj_path.to_str().unwrap(),
            ])
            .output();

        match clang_output {
            Ok(output) => {
                if !output.status.success() {
                    eprintln!("clang error:\n{}", String::from_utf8_lossy(&output.stderr));
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("Failed to run clang: {}", e);
                std::process::exit(1);
            }
        }
    }

    // let _ = fs::remove_file(&ll_path);
    // let _ = fs::remove_file(&obj_path);

    println!("✓ {} generated successfully!", exe_path.display());
}
