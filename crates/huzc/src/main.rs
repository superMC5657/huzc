use clap::Parser;
use huzi_codegen::CodeGen;
use huzi_lexer::Lexer;
use huzi_parser::Parser as HuziParser;
use inkwell::context::Context;
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Parser, Debug)]
#[command(name = "huzc")]
#[command(about = "Huzi Programming Language Compiler")]
struct Args {
    #[arg(short, long)]
    input: String,

    #[arg(short, long, default_value = "a.exe")]
    output: String,

    #[arg(long, default_value = "false")]
    emit_llvm: bool,

    #[arg(long, default_value = "false")]
    only_compile: bool,
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

    let input_path = Path::new(&args.input);
    let stem = input_path.file_stem().unwrap().to_str().unwrap();
    let temp_dir = std::env::temp_dir();
    let ll_path = temp_dir.join(format!("{}.ll", stem));
    let obj_path = temp_dir.join(format!("{}.obj", stem));

    if args.emit_llvm {
        println!("[5/5] Writing LLVM IR...");
        if let Err(e) = codegen.write_ir_to_file(&args.output) {
            eprintln!("Error writing IR: {}", e);
            std::process::exit(1);
        }
        println!("✓ LLVM IR written to {}", args.output);
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
        if let Err(e) = fs::copy(&obj_path, &args.output) {
            eprintln!("Error copying object file: {}", e);
            std::process::exit(1);
        }
        println!("✓ Object file written to {}", args.output);

        let _ = fs::remove_file(&ll_path);
        let _ = fs::remove_file(&obj_path);
        return;
    }

    println!("  Linking to executable...");

    // Try lld-link first, then fall back to clang
    let lld_output = Command::new("lld-link")
        .args([
            &format!("/OUT:{}", &args.output),
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
                &args.output,
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

    let _ = fs::remove_file(&ll_path);
    let _ = fs::remove_file(&obj_path);

    println!("✓ {} generated successfully!", args.output);
}
