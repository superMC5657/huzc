//! 模块加载:处理 import 语句,加载被导入模块的源码,做路径解析、
//! 去重与循环导入检测。加载结果交给 codegen 的 add_module 注册。

use crate::die;
use huzi_ast::{Program, Stmt};
use huzi_codegen::BUILTIN_MODULES;
use huzi_lexer::Lexer;
use huzi_parser::Parser as HuziParser;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 一个已加载的模块。
pub struct LoadedModule {
    /// 符号绑定名(点分 import 名的末段)。
    pub name: String,
    /// 解析后的 AST;内置模块(如 math)为 None。
    pub program: Option<Program>,
    /// 模块源文件路径;内置模块为 None,供调试信息生成 DIFile。
    pub path: Option<PathBuf>,
}

/// 全加载过程共享的状态:已解析模块(防循环导入)与加载结果。
/// `loaded` 必须跨递归共享,否则循环导入会无限递归。
struct LoadState {
    loaded: HashMap<String, PathBuf>,
    modules: Vec<LoadedModule>,
}

/// 加载主程序的全部 import,并把 Import 语句从程序中移除。
/// 返回模块列表;内置模块的 AST 与路径均为 None。
pub fn load_modules(program: &mut Program, base_dir: &Path) -> Vec<LoadedModule> {
    let import_names = extract_imports(program);

    let mut state = LoadState {
        loaded: HashMap::new(),
        modules: Vec::new(),
    };
    for name in import_names {
        load_module(&name, base_dir, &mut state);
    }
    state.modules
}

/// 取出主程序中的 import 语句并从语句列表中移除。
fn extract_imports(program: &mut Program) -> Vec<String> {
    let mut names = Vec::new();
    program.statements.retain(|stmt| match &stmt.node {
        Stmt::Import(imp) => {
            names.push(imp.name.clone());
            false
        }
        _ => true,
    });
    names
}

/// import 名的符号绑定名:点分路径取末段(`mods.helpers` -> `helpers`)。
fn module_bind_name(import_name: &str) -> &str {
    import_name.rsplit('.').next().unwrap_or(import_name)
}

/// 解析并加载单个模块(递归处理其自身依赖)。
fn load_module(import_name: &str, base_dir: &Path, state: &mut LoadState) {
    let name = module_bind_name(import_name).to_string();
    if state.modules.iter().any(|m| m.name == name) {
        return;
    }

    // 内置模块:不解析文件,由 codegen 走 builtin 调度。
    if BUILTIN_MODULES.contains(&name.as_str()) {
        state.modules.push(LoadedModule {
            name,
            program: None,
            path: None,
        });
        return;
    }

    let path = resolve_module_file(import_name, base_dir);
    if let Some(prev) = state.loaded.get(&name) {
        if prev != &path {
            die(format!(
                "Module '{}' resolves to different files: {} and {}",
                name,
                prev.display(),
                path.display()
            ));
        }
        return;
    }
    state.loaded.insert(name.clone(), path.clone());

    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| die(format!("Error reading module '{}': {}", path.display(), e)));

    let tokens = Lexer::new(source.clone())
        .tokenize()
        .unwrap_or_else(|e| die(huzi_error::render(&e, &source, &format!("Module '{}' error", name))));
    let mut module_program = HuziParser::new(tokens)
        .parse()
        .unwrap_or_else(|e| die(huzi_error::render(&e, &source, &format!("Module '{}' error", name))));

    validate_module_program(&name, &module_program);

    // 模块自身的 import 相对模块文件所在目录解析,与主程序共享状态。
    let module_dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let nested = extract_imports(&mut module_program);
    for nested_name in nested {
        load_module(&nested_name, &module_dir, state);
    }
    state.modules.push(LoadedModule {
        name,
        program: Some(module_program),
        path: Some(path),
    });
}

/// 模块名 -> 文件:点分段转子路径加 `.hz`,先找导入文件同目录,再找当前工作目录。
fn resolve_module_file(import_name: &str, base_dir: &Path) -> PathBuf {
    let segments: PathBuf = import_name.split('.').collect();
    let file = segments.with_extension("hz");
    for dir in [base_dir, Path::new(".")] {
        let candidate = dir.join(&file);
        if candidate.is_file() {
            return candidate;
        }
    }
    die(format!(
        "Cannot find module '{}': tried {} and {}",
        import_name,
        base_dir.join(&file).display(),
        Path::new(".").join(&file).display()
    ))
}

/// 模块文件只允许定义(fn/struct/enum)与 import,不允许顶层级语句。
fn validate_module_program(name: &str, program: &Program) {
    for stmt in &program.statements {
        if !matches!(
            &stmt.node,
            Stmt::Fn(_) | Stmt::Struct(_) | Stmt::Enum(_) | Stmt::Import(_)
        ) {
            die(format!(
                "Module '{}' may only contain fn/struct/enum definitions and imports",
                name
            ));
        }
    }
}
