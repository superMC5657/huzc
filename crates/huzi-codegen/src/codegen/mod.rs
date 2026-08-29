use huzi_ast::*;
use huzi_error::HuziError;
use huzi_error::Result;
use std::collections::HashMap;
use inkwell::{
    builder::Builder,
    context::Context,
    module::Module,
    types::BasicType,
    values::{FunctionValue, PointerValue},
};
mod aggregates;
mod builtins;
mod expr;
mod stmt;
#[cfg(test)]
mod tests;
mod tuples;
mod types;


/// A variable slot: `ptr` always holds a pointer whose loaded value has type
/// `ty`. For arrays, `ptr` holds the address of the array data (loaded as a
/// `ptr`), and `elem` records the element type for GEP/indexing.
#[derive(Clone, Copy)]
struct VarSlot<'ctx> {
    ptr: PointerValue<'ctx>,
    ty: inkwell::types::BasicTypeEnum<'ctx>,
    elem: Option<inkwell::types::BasicTypeEnum<'ctx>>,
    array_len: Option<u32>,
    mutable: bool,
}

/// A registered struct field. `ast_ty` keeps the original AST type because
/// array fields decay to bare pointers in LLVM and would lose their element
/// type.
#[derive(Clone)]
struct StructFieldInfo<'ctx> {
    name: String,
    ty: inkwell::types::BasicTypeEnum<'ctx>,
    ast_ty: Type,
}

#[derive(Clone)]
struct EnumVariantInfo<'ctx> {
    name: String,
    /// Discriminant value, equal to the variant's declaration index.
    tag: u32,
    /// LLVM type of the payload; None for unit variants.
    payload: Option<inkwell::types::BasicTypeEnum<'ctx>>,
    /// AST payload type (retains array element types, like StructFieldInfo).
    ast_payload: Option<Type>,
    /// Index of this variant's payload inside the payload-union struct.
    payload_slot: Option<u32>,
}

#[derive(Clone)]
struct EnumInfo<'ctx> {
    name: String,
    variants: Vec<EnumVariantInfo<'ctx>>,
    /// Data-carrying enums are laid out as { i32 tag, payload union }. Simple
    /// enums are represented directly as their i32 tag (None here).
    llvm: Option<inkwell::types::StructType<'ctx>>,
    payload_union: Option<inkwell::types::StructType<'ctx>>,
}

pub struct CodeGen<'ctx> {
    context: &'ctx Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
    scopes: Vec<HashMap<String, VarSlot<'ctx>>>,
    functions: HashMap<String, (FunctionValue<'ctx>, Vec<inkwell::types::BasicTypeEnum<'ctx>>)>,
    current_return_type: Option<inkwell::types::BasicTypeEnum<'ctx>>,
    /// (continue_target, break_target) for each enclosing loop.
    loop_stack: Vec<(inkwell::basic_block::BasicBlock<'ctx>, inkwell::basic_block::BasicBlock<'ctx>)>,
    /// Registered user-defined structs: name -> (LLVM type, ordered fields).
    structs: HashMap<
        String,
        (
            inkwell::types::StructType<'ctx>,
            Vec<StructFieldInfo<'ctx>>,
        ),
    >,
    /// Registered user-defined enums: name -> layout info.
    enums: HashMap<String, EnumInfo<'ctx>>,
}
impl<'ctx> CodeGen<'ctx> {
    pub fn new(context: &'ctx Context, name: &str) -> Self {
        let module = context.create_module(name);
        let builder = context.create_builder();

        Self {
            context,
            module,
            builder,
            scopes: vec![HashMap::new()],
            functions: HashMap::new(),
            current_return_type: None,
            loop_stack: Vec::new(),
            structs: HashMap::new(),
            enums: HashMap::new(),
        }
    }

    pub fn compile(&mut self, program: &Program) -> Result<()> {
        self.prelude()?;

        let fn_stmts = self.register_program_types(program)?;
        self.declare_fn_signatures(&fn_stmts)?;

        for fn_stmt in &fn_stmts {
            self.compile_fn(fn_stmt)?;
        }

        self.compile_top_level(program, &fn_stmts)?;

        Ok(())
    }

    /// Register all top-level struct/enum definitions before anything else
    /// so function signatures and field types can reference them. Returns
    /// the collected function definitions.
    fn register_program_types(&mut self, program: &Program) -> Result<Vec<FnStmt>> {
        let struct_defs: Vec<StructDef> = program
            .statements
            .iter()
            .filter_map(|s| match s {
                Stmt::Struct(d) => Some(d.clone()),
                _ => None,
            })
            .collect();
        let enum_defs: Vec<EnumDef> = program
            .statements
            .iter()
            .filter_map(|s| match s {
                Stmt::Enum(d) => Some(d.clone()),
                _ => None,
            })
            .collect();
        self.check_type_cycles(&struct_defs, &enum_defs)?;
        self.register_struct_names(&struct_defs)?;
        self.register_enum_names(&enum_defs)?;
        self.resolve_struct_bodies(&struct_defs)?;
        self.resolve_enum_bodies(&enum_defs)?;

        Ok(program
            .statements
            .iter()
            .filter_map(|s| match s {
                Stmt::Fn(f) => Some(f.clone()),
                _ => None,
            })
            .collect())
    }

    fn declare_fn_signatures(&mut self, fn_stmts: &[FnStmt]) -> Result<()> {
        for fn_stmt in fn_stmts {
            self.compile_fn_signature(fn_stmt)?;
        }
        Ok(())
    }

    /// Top-level statements must live in a `main` function; synthesize one
    /// if the program only has top-level code.
    fn compile_top_level(&mut self, program: &Program, fn_stmts: &[FnStmt]) -> Result<()> {
        let has_main = fn_stmts.iter().any(|f| f.name == "main");
        let top_level: Vec<&Stmt> = program
            .statements
            .iter()
            .filter(|s| !matches!(s, Stmt::Fn(_) | Stmt::Struct(_) | Stmt::Enum(_)))
            .collect();

        if has_main {
            if !top_level.is_empty() {
                return Err(HuziError::new_global(
                    "Cannot mix top-level statements with `fn main`; move the top-level code into a function",
                ));
            }
            return Ok(());
        }

        if top_level.is_empty() {
            return Err(HuziError::new_global(
                "No `fn main` found; define `fn main() -> i32 { ... }` or write top-level statements",
            ));
        }

        let main_type = self.context.i32_type().fn_type(&[], false);
        let main_fn = self.module.add_function("main", main_type, None);
        self.functions.insert("main".to_string(), (main_fn, vec![]));

        let entry = self.context.append_basic_block(main_fn, "entry");
        self.builder.position_at_end(entry);
        self.emit_console_utf8_setup();
        self.current_return_type = Some(self.context.i32_type().into());
        self.scopes = vec![HashMap::new()];

        for stmt in &top_level {
            self.compile_stmt(stmt)?;
        }

        if self.at_open_end() {
            self.builder
                .build_return(Some(&self.context.i32_type().const_int(0, false)))
                .unwrap();
        }

        Ok(())
    }

    fn compile_fn_signature(&mut self, stmt: &FnStmt) -> Result<()> {
        let param_types: Vec<inkwell::types::BasicMetadataTypeEnum<'ctx>> = stmt
            .params
            .iter()
            .map(|p| self.type_to_llvm(&p.param_type).map(|t| t.into()))
            .collect::<Result<Vec<_>>>()?;

        let fn_type = if let Some(ret_type) = &stmt.return_type {
            self.type_to_llvm(ret_type)?.fn_type(&param_types, false)
        } else {
            self.context.i32_type().fn_type(&param_types, false)
        };

        let function = self.module.add_function(&stmt.name, fn_type, None);
        let param_llvm_types: Vec<inkwell::types::BasicTypeEnum<'ctx>> = stmt
            .params
            .iter()
            .map(|p| self.type_to_llvm(&p.param_type))
            .collect::<Result<Vec<_>>>()?;
        self.functions
            .insert(stmt.name.clone(), (function, param_llvm_types));

        Ok(())
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn scope_insert(&mut self, name: String, slot: VarSlot<'ctx>) {
        self.scopes
            .last_mut()
            .expect("scope stack is never empty")
            .insert(name, slot);
    }

    fn scope_lookup(&self, name: &str) -> Option<VarSlot<'ctx>> {
        for scope in self.scopes.iter().rev() {
            if let Some(slot) = scope.get(name) {
                return Some(*slot);
            }
        }
        None
    }

    fn current_function(&self) -> Result<FunctionValue<'ctx>> {
        self.builder
            .get_insert_block()
            .and_then(|b| b.get_parent())
            .ok_or_else(|| HuziError::new_global("No current function"))
    }

    // ==================== Diagnostics ====================

    /// 收集当前可见的全部名字(函数、结构体、枚举、作用域变量),
    /// 用于未知名字的 "did you mean" 建议。
    fn visible_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.functions.keys().map(|s| s.as_str()).collect();
        names.extend(self.structs.keys().map(|s| s.as_str()));
        names.extend(self.enums.keys().map(|s| s.as_str()));
        for scope in &self.scopes {
            names.extend(scope.keys().map(|s| s.as_str()));
        }
        names
    }

    /// 构造 "Unknown variable" 错误,附最接近名字的修复建议。
    pub(super) fn unknown_variable_error(&self, name: &str) -> HuziError {
        let mut message = format!("Unknown variable: {}", name);
        if let Some(hint) = huzi_error::did_you_mean(name, self.visible_names()) {
            message.push_str(&format!("\n  help: {}", hint));
        }
        HuziError::new_global(message)
    }

    /// 构造 "Unknown function" 错误,附最接近名字的修复建议。
    pub(super) fn unknown_function_error(&self, name: &str) -> HuziError {
        let mut message = format!("Unknown function: {}", name);
        if let Some(hint) = huzi_error::did_you_mean(name, self.visible_names()) {
            message.push_str(&format!("\n  help: {}", hint));
        }
        HuziError::new_global(message)
    }

    /// True if the current insert block has no terminator yet.
    fn at_open_end(&self) -> bool {
        self.builder
            .get_insert_block()
            .map(|b| b.get_terminator().is_none())
            .unwrap_or(false)
    }

    // ==================== Statements ====================
}

impl<'ctx> CodeGen<'ctx> {
    pub fn print_llvm_ir(&self) -> String {
        self.module.print_to_string().to_string()
    }

    pub fn verify(&self) -> bool {
        self.module.verify().is_ok()
    }

    pub fn write_ir_to_file(&self, path: &str) -> std::result::Result<(), std::io::Error> {
        std::fs::write(path, self.module.print_to_string().to_string())
    }
}
