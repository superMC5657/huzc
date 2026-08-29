use clap::Parser;

/// Linker/toolchain to use for linking the final executable
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum LinkerKind {
    /// lld-link with MSVC/Windows SDK (auto-detects SDK libs)
    Msvc,
    /// clang driver as linker
    Clang,
    /// MinGW-w64 (gcc driver)
    Mingw,
}

impl std::fmt::Display for LinkerKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            LinkerKind::Msvc => "msvc",
            LinkerKind::Clang => "clang",
            LinkerKind::Mingw => "mingw",
        };
        f.write_str(s)
    }
}

impl LinkerKind {
    /// Platform default: msvc on Windows, clang elsewhere.
    pub fn platform_default() -> Self {
        if cfg!(target_os = "windows") {
            LinkerKind::Msvc
        } else {
            LinkerKind::Clang
        }
    }
}

/// Huzi Programming Language Compiler
#[derive(Parser, Debug)]
#[command(name = "huzc")]
#[command(about = "Compile Huzi source code to executable")]
pub struct Args {
    /// Input source file (.hz)
    #[arg(short, long)]
    pub input: String,

    /// Output file name (without extension)
    #[arg(short, long, default_value = "a")]
    pub output: String,

    /// Linker to use (defaults to msvc on Windows, clang on macOS/Linux)
    #[arg(long, value_enum, default_value_t = LinkerKind::platform_default())]
    pub linker: LinkerKind,
}
