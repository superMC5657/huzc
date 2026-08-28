use huzi_ast::*;
use huzi_error::HuziError;
use huzi_error::Result;
use inkwell::{
    builder::Builder,
    context::Context,
    module::Module,
    types::BasicType,
    values::{FunctionValue, PointerValue},
    AddressSpace,
};
use std::collections::HashMap;

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

pub struct CodeGen<'ctx> {
    context: &'ctx Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
    scopes: Vec<HashMap<String, VarSlot<'ctx>>>,
    functions: HashMap<String, (FunctionValue<'ctx>, Vec<inkwell::types::BasicTypeEnum<'ctx>>)>,
    current_return_type: Option<inkwell::types::BasicTypeEnum<'ctx>>,
    /// (continue_target, break_target) for each enclosing loop.
    loop_stack: Vec<(inkwell::basic_block::BasicBlock<'ctx>, inkwell::basic_block::BasicBlock<'ctx>)>,
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
        }
    }

    pub fn compile(&mut self, program: &Program) -> Result<()> {
        self.prelude()?;

        let fn_stmts: Vec<FnStmt> = program
            .statements
            .iter()
            .filter_map(|s| match s {
                Stmt::Fn(f) => Some(f.clone()),
                _ => None,
            })
            .collect();

        for fn_stmt in &fn_stmts {
            self.compile_fn_signature(fn_stmt)?;
        }

        for fn_stmt in &fn_stmts {
            self.compile_fn(fn_stmt)?;
        }

        // Top-level statements must live in a `main` function; synthesize one
        // if the program only has top-level code.
        let has_main = fn_stmts.iter().any(|f| f.name == "main");
        let top_level: Vec<&Stmt> = program
            .statements
            .iter()
            .filter(|s| !matches!(s, Stmt::Fn(_)))
            .collect();

        if has_main {
            if !top_level.is_empty() {
                return Err(HuziError::new_global(
                    "Cannot mix top-level statements with `fn main`; move the top-level code into a function",
                ));
            }
        } else {
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

    fn prelude(&mut self) -> Result<()> {
        // printf for print function
        let print_fn = self.context.i32_type().fn_type(
            &[self
                .context
                .ptr_type(AddressSpace::default())
                .into()],
            true,
        );
        self.module.add_function("printf", print_fn, None);

        // scanf for input functions
        let scanf_fn = self.context.i32_type().fn_type(
            &[self
                .context
                .ptr_type(AddressSpace::default())
                .into()],
            true,
        );
        self.module.add_function("scanf", scanf_fn, None);

        // getchar for read_line
        let getchar_fn = self.context.i32_type().fn_type(&[], false);
        self.module.add_function("getchar", getchar_fn, None);

        // malloc for string allocation (returns i8*)
        let malloc_fn = self.context.ptr_type(inkwell::AddressSpace::default()).fn_type(
            &[self.context.i32_type().into()],
            false,
        );
        self.module.add_function("malloc", malloc_fn, None);

        // sprintf for to_string
        let sprintf_fn = self.context.i32_type().fn_type(
            &[
                self.context.ptr_type(AddressSpace::default()).into(),
                self.context.ptr_type(AddressSpace::default()).into(),
            ],
            true,
        );
        self.module.add_function("sprintf", sprintf_fn, None);

        // Math functions (link to libm)
        let sqrt_fn = self.context.f64_type().fn_type(&[self.context.f64_type().into()], false);
        self.module.add_function("sqrt", sqrt_fn, None);

        let pow_fn = self.context.f64_type().fn_type(
            &[
                self.context.f64_type().into(),
                self.context.f64_type().into(),
            ],
            false,
        );
        self.module.add_function("pow", pow_fn, None);

        let sin_fn = self.context.f64_type().fn_type(&[self.context.f64_type().into()], false);
        self.module.add_function("sin", sin_fn, None);

        let cos_fn = self.context.f64_type().fn_type(&[self.context.f64_type().into()], false);
        self.module.add_function("cos", cos_fn, None);

        let fabs_fn = self.context.f64_type().fn_type(&[self.context.f64_type().into()], false);
        self.module.add_function("fabs", fabs_fn, None);

        // strlen for string length
        let strlen_fn = self.context.i32_type().fn_type(
            &[self
                .context
                .ptr_type(AddressSpace::default())
                .into()],
            false,
        );
        self.module.add_function("strlen", strlen_fn, None);

        // strcpy for string copy
        let strcpy_fn = self.context.i32_type().fn_type(
            &[
                self.context.ptr_type(AddressSpace::default()).into(),
                self.context.ptr_type(AddressSpace::default()).into(),
            ],
            false,
        );
        self.module.add_function("strcpy", strcpy_fn, None);

        Ok(())
    }

    // ==================== Scope helpers ====================

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

    /// True if the current insert block has no terminator yet.
    fn at_open_end(&self) -> bool {
        self.builder
            .get_insert_block()
            .map(|b| b.get_terminator().is_none())
            .unwrap_or(false)
    }

    // ==================== Statements ====================

    fn compile_stmt(&mut self, stmt: &Stmt) -> Result<()> {
        match stmt {
            Stmt::Let(let_stmt) => self.compile_let(let_stmt),
            Stmt::Fn(fn_stmt) => self.compile_fn(fn_stmt),
            Stmt::Expr(expr_stmt) => {
                self.compile_expr(&expr_stmt.expr)?;
                Ok(())
            }
            Stmt::Return(return_stmt) => self.compile_return(return_stmt),
            Stmt::Break => self.compile_break(),
            Stmt::Continue => self.compile_continue(),
            Stmt::Block(block) => self.compile_block(block),
            Stmt::If(if_stmt) => self.compile_if(if_stmt),
            Stmt::For(for_stmt) => self.compile_for(for_stmt),
            Stmt::While(while_stmt) => self.compile_while(while_stmt),
        }
    }

    fn compile_let(&mut self, stmt: &LetStmt) -> Result<()> {
        match &stmt.value {
            Some(Expr::ArrayLiteral(elements)) => {
                if elements.is_empty() {
                    return Err(HuziError::new_global("Empty array literal not supported"));
                }

                let mut values = Vec::with_capacity(elements.len());
                for e in elements {
                    values.push(self.compile_expr(e)?);
                }

                let elem_type = values[0].get_type();
                let array_type = elem_type.array_type(values.len() as u32);
                let array_ptr = self.build_alloca(array_type.into(), &stmt.name)?;

                for (i, val) in values.iter().enumerate() {
                    let val = self.coerce_value(elem_type, *val)?;
                    let index = self.context.i32_type().const_int(i as u64, false);
                    let elem_ptr = unsafe {
                        self.builder
                            .build_gep(elem_type, array_ptr, &[index], "arr_elem")
                            .unwrap()
                    };
                    self.builder.build_store(elem_ptr, val).unwrap();
                }

                // Store the array address in a pointer slot so loading the
                // variable yields the array address.
                let ptr_ty = self.context.ptr_type(AddressSpace::default());
                let slot_ptr = self.build_alloca(ptr_ty.into(), &format!("{}.ptr", stmt.name))?;
                self.builder.build_store(slot_ptr, array_ptr).unwrap();
                self.scope_insert(
                    stmt.name.clone(),
                    VarSlot {
                        ptr: slot_ptr,
                        ty: ptr_ty.into(),
                        elem: Some(elem_type),
                        array_len: Some(values.len() as u32),
                        mutable: stmt.mutable,
                    },
                );
            }
            Some(value_expr) => {
                let mut value = self.compile_expr(value_expr)?;

                let var_type = match &stmt.type_annotation {
                    Some(t) => {
                        let ty = self.type_to_llvm(t)?;
                        value = self.coerce_value(ty, value)?;
                        ty
                    }
                    None => value.get_type(),
                };

                let alloca = self.build_alloca(var_type, &stmt.name)?;
                self.builder.build_store(alloca, value).unwrap();

                // Pointers to strings support char indexing.
                let elem = if var_type.is_pointer_type() {
                    Some(self.context.i8_type().into())
                } else {
                    None
                };

                self.scope_insert(
                    stmt.name.clone(),
                    VarSlot {
                        ptr: alloca,
                        ty: var_type,
                        elem,
                        array_len: None,
                        mutable: stmt.mutable,
                    },
                );
            }
            None => {
                // Declaration without initializer: requires a type annotation.
                let ty = match &stmt.type_annotation {
                    Some(t) => self.type_to_llvm(t)?,
                    None => {
                        return Err(HuziError::new_global(format!(
                            "Variable '{}' declared without a value or a type annotation",
                            stmt.name
                        )))
                    }
                };
                let alloca = self.build_alloca(ty, &stmt.name)?;
                self.builder.build_store(alloca, ty.const_zero()).unwrap();

                let elem = if ty.is_pointer_type() {
                    Some(self.context.i8_type().into())
                } else {
                    None
                };

                self.scope_insert(
                    stmt.name.clone(),
                    VarSlot {
                        ptr: alloca,
                        ty,
                        elem,
                        array_len: None,
                        mutable: stmt.mutable,
                    },
                );
            }
        }
        Ok(())
    }

    fn compile_fn(&mut self, stmt: &FnStmt) -> Result<()> {
        let (function, _) = self
            .functions
            .get(&stmt.name)
            .cloned()
            .ok_or_else(|| HuziError::new_global(format!("Unknown function: {}", stmt.name)))?;

        let entry = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry);

        let return_type = match &stmt.return_type {
            Some(t) => self.type_to_llvm(t)?,
            None => self.context.i32_type().into(),
        };
        self.current_return_type = Some(return_type);
        self.scopes = vec![HashMap::new()];

        for (i, param) in stmt.params.iter().enumerate() {
            let arg = function.get_nth_param(i as u32).unwrap();
            let arg_type = arg.get_type();

            let alloca = self.build_alloca(arg_type, &param.name)?;
            self.builder.build_store(alloca, arg).unwrap();

            // Arrays decay to pointers; remember the element type for indexing.
            let elem = match &param.param_type {
                Type::Array(elem_ty, _) => Some(self.type_to_llvm(elem_ty)?),
                Type::Str => Some(self.context.i8_type().into()),
                _ => None,
            };

            self.scopes.last_mut().unwrap().insert(
                param.name.clone(),
                VarSlot {
                    ptr: alloca,
                    ty: arg_type,
                    elem,
                    array_len: match &param.param_type {
                        Type::Array(_, size) => Some(*size as u32),
                        _ => None,
                    },
                    mutable: true,
                },
            );
        }

        self.compile_block(&stmt.body)?;

        // Functions without an explicit return fall through with a zero value
        // of the declared return type.
        if self.at_open_end() {
            self.builder
                .build_return(Some(&return_type.const_zero()))
                .unwrap();
        }

        self.current_return_type = None;

        Ok(())
    }

    fn compile_return(&mut self, stmt: &ReturnStmt) -> Result<()> {
        let ret_type = self
            .current_return_type
            .unwrap_or_else(|| self.context.i32_type().into());

        match &stmt.value {
            Some(value) => {
                let value = self.compile_expr(value)?;
                let value = self.coerce_value(ret_type, value)?;
                self.builder.build_return(Some(&value)).unwrap();
            }
            None => {
                self.builder
                    .build_return(Some(&ret_type.const_zero()))
                    .unwrap();
            }
        }
        Ok(())
    }

    fn compile_block(&mut self, block: &Block) -> Result<()> {
        self.push_scope();
        for stmt in &block.statements {
            self.compile_stmt(stmt)?;
        }
        self.pop_scope();
        Ok(())
    }

    /// Compile a block as an expression: the block's value is the value of its
    /// last expression statement.
    fn compile_block_value(&mut self, block: &Block) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        self.push_scope();
        let mut last: Option<inkwell::values::BasicValueEnum<'ctx>> = None;
        for stmt in &block.statements {
            match stmt {
                Stmt::Expr(es) => last = Some(self.compile_expr(&es.expr)?),
                other => self.compile_stmt(other)?,
            }
        }
        self.pop_scope();
        last.ok_or_else(|| HuziError::new_global("Block used as an expression must end with a value"))
    }

    fn compile_if(&mut self, stmt: &IfStmt) -> Result<()> {
        // Fold the elif chain into nested if/else so each branch is compiled.
        let else_block: Option<Block> = if stmt.elif_branches.is_empty() {
            stmt.else_branch.clone()
        } else {
            let nested = Self::fold_elif(&stmt.elif_branches, stmt.else_branch.as_ref());
            Some(Block {
                statements: vec![Stmt::If(nested)],
            })
        };

        self.compile_branch(&stmt.condition, &stmt.then_branch, else_block.as_ref())
    }

    fn fold_elif(elifs: &[(Expr, Block)], else_b: Option<&Block>) -> IfStmt {
        let (first, rest) = elifs.split_first().expect("elif list is not empty");
        let inner_else = if rest.is_empty() {
            else_b.cloned()
        } else {
            Some(Block {
                statements: vec![Stmt::If(Self::fold_elif(rest, else_b))],
            })
        };
        IfStmt {
            condition: first.0.clone(),
            then_branch: first.1.clone(),
            elif_branches: Vec::new(),
            else_branch: inner_else,
        }
    }

    fn compile_branch(
        &mut self,
        condition: &Expr,
        then_b: &Block,
        else_b: Option<&Block>,
    ) -> Result<()> {
        let cond_value = self.compile_expr(condition)?;
        let cond = self.to_i1(cond_value)?;

        let function = self.current_function()?;

        let then_block = self.context.append_basic_block(function, "then");
        let else_block = self.context.append_basic_block(function, "else");
        let merge_block = self.context.append_basic_block(function, "merge");

        self.builder
            .build_conditional_branch(cond, then_block, else_block)
            .unwrap();

        self.builder.position_at_end(then_block);
        self.compile_block(then_b)?;
        let then_open = self.at_open_end();
        if then_open {
            self.builder
                .build_unconditional_branch(merge_block)
                .unwrap();
        }

        self.builder.position_at_end(else_block);
        if let Some(else_branch) = else_b {
            self.compile_block(else_branch)?;
        }
        let else_open = self.at_open_end();
        if else_open {
            self.builder
                .build_unconditional_branch(merge_block)
                .unwrap();
        }

        self.builder.position_at_end(merge_block);

        // If every branch returned, the merge block is unreachable.
        if !then_open && !else_open {
            self.builder.build_unreachable().unwrap();
        }

        Ok(())
    }

    fn compile_for(&mut self, stmt: &ForStmt) -> Result<()> {
        let i_type = self.context.i32_type();

        let start = self.compile_expr(&stmt.start)?;
        let start = match self.coerce_value(i_type.into(), start)? {
            inkwell::values::BasicValueEnum::IntValue(iv) => iv,
            _ => return Err(HuziError::new_global("for loop start must be an integer")),
        };

        let end = self.compile_expr(&stmt.end)?;
        let end = match self.coerce_value(i_type.into(), end)? {
            inkwell::values::BasicValueEnum::IntValue(iv) => iv,
            _ => return Err(HuziError::new_global("for loop end must be an integer")),
        };

        let function = self.current_function()?;

        let loop_block = self.context.append_basic_block(function, "for_loop");
        let body_block = self.context.append_basic_block(function, "for_body");
        let after_block = self.context.append_basic_block(function, "for_after");

        self.loop_stack.push((loop_block, after_block));

        // Allocate and initialize the loop variable.
        let i_alloca = self.build_alloca(i_type.into(), &stmt.var_name)?;
        self.builder.build_store(i_alloca, start).unwrap();

        self.builder
            .build_unconditional_branch(loop_block)
            .unwrap();

        // Condition check.
        self.builder.position_at_end(loop_block);
        let i = self
            .builder
            .build_load(i_type, i_alloca, "i")
            .unwrap()
            .into_int_value();
        let condition = self
            .builder
            .build_int_compare(inkwell::IntPredicate::SLT, i, end, "loop_cond")
            .unwrap();
        self.builder
            .build_conditional_branch(condition, body_block, after_block)
            .unwrap();

        // Body.
        self.builder.position_at_end(body_block);
        self.push_scope();
        self.scope_insert(
            stmt.var_name.clone(),
            VarSlot {
                ptr: i_alloca,
                ty: i_type.into(),
                elem: None,
                array_len: None,
                mutable: true,
            },
        );
        self.compile_block(&stmt.body)?;
        self.pop_scope();

        // Increment the loop variable before jumping back to the condition.
        let i = self
            .builder
            .build_load(i_type, i_alloca, "i")
            .unwrap()
            .into_int_value();
        let i_next = self
            .builder
            .build_int_add(i, i_type.const_int(1, false), "i_next")
            .unwrap();
        self.builder.build_store(i_alloca, i_next).unwrap();
        self.builder
            .build_unconditional_branch(loop_block)
            .unwrap();

        self.loop_stack.pop();

        // Continue after the loop.
        self.builder.position_at_end(after_block);

        Ok(())
    }

    fn compile_break(&mut self) -> Result<()> {
        let (_, break_target) = *self
            .loop_stack
            .last()
            .ok_or_else(|| HuziError::new_global("`break` outside of a loop"))?;
        self.builder
            .build_unconditional_branch(break_target)
            .unwrap();
        self.start_dead_block()
    }

    fn compile_continue(&mut self) -> Result<()> {
        let (continue_target, _) = *self
            .loop_stack
            .last()
            .ok_or_else(|| HuziError::new_global("`continue` outside of a loop"))?;
        self.builder
            .build_unconditional_branch(continue_target)
            .unwrap();
        self.start_dead_block()
    }

    /// After a break/continue the current block is terminated; move to a fresh
    /// block so following statements still have somewhere to go.
    fn start_dead_block(&mut self) -> Result<()> {
        let function = self.current_function()?;
        let dead = self.context.append_basic_block(function, "dead");
        self.builder.position_at_end(dead);
        Ok(())
    }

    fn compile_while(&mut self, stmt: &WhileStmt) -> Result<()> {
        let function = self.current_function()?;

        let cond_block = self.context.append_basic_block(function, "while_cond");
        let body_block = self.context.append_basic_block(function, "while_body");
        let after_block = self.context.append_basic_block(function, "while_after");

        self.loop_stack.push((cond_block, after_block));

        self.builder
            .build_unconditional_branch(cond_block)
            .unwrap();

        // Re-evaluate the condition on every iteration.
        self.builder.position_at_end(cond_block);
        let cond_value = self.compile_expr(&stmt.condition)?;
        let condition = self.to_i1(cond_value)?;
        self.builder
            .build_conditional_branch(condition, body_block, after_block)
            .unwrap();

        self.builder.position_at_end(body_block);
        self.compile_block(&stmt.body)?;
        self.builder
            .build_unconditional_branch(cond_block)
            .unwrap();

        self.loop_stack.pop();

        self.builder.position_at_end(after_block);

        Ok(())
    }

    // ==================== Expressions ====================

    fn compile_expr(&mut self, expr: &Expr) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        match expr {
            Expr::Literal(lit) => self.compile_literal(lit),
            Expr::Ident(name) => match self.scope_lookup(name) {
                Some(slot) => {
                    let loaded = self
                        .builder
                        .build_load(slot.ty, slot.ptr, "load")
                        .unwrap();
                    Ok(loaded)
                }
                None => Err(HuziError::new_global(format!("Unknown variable: {}", name))),
            },
            Expr::Binary(bin_expr) => self.compile_binary(bin_expr),
            Expr::Unary(unary_expr) => self.compile_unary(unary_expr),
            Expr::Call(call_expr) => self.compile_call(call_expr),
            Expr::Assign(assign_expr) => self.compile_assign(assign_expr),
            Expr::ArrayIndex(idx_expr) => self.compile_array_index(idx_expr),
            Expr::ArrayLiteral(elements) => self.compile_array_literal(elements),
            Expr::If(if_expr) => self.compile_if_expr(if_expr),
        }
    }

    fn compile_literal(&self, lit: &Literal) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        match lit {
            Literal::Int(n) => {
                // Integers that fit in i32 use i32; larger ones use i64.
                if *n >= i32::MIN as i64 && *n <= i32::MAX as i64 {
                    Ok(self.context.i32_type().const_int(*n as u64, false).into())
                } else {
                    Ok(self.context.i64_type().const_int(*n as u64, false).into())
                }
            }
            Literal::Float(f) => Ok(self.context.f64_type().const_float(*f).into()),
            Literal::Bool(b) => Ok(self.context.bool_type().const_int(*b as u64, false).into()),
            Literal::String(s) => {
                let g = unsafe { self.builder.build_global_string(s, "str").unwrap() };
                Ok(g.as_pointer_value().into())
            }
            Literal::Char(c) => Ok(self.context.i8_type().const_int(*c as u64, false).into()),
        }
    }

    fn compile_binary(
        &mut self,
        expr: &BinaryExpr,
    ) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        // && and || short-circuit; handle them before evaluating operands.
        match expr.operator {
            BinOp::And => return self.compile_short_circuit(&expr.left, &expr.right, true),
            BinOp::Or => return self.compile_short_circuit(&expr.left, &expr.right, false),
            _ => {}
        }

        let mut left = self.compile_expr(&expr.left)?;
        let mut right = self.compile_expr(&expr.right)?;

        // Mixed int/float: convert the int operand to the float operand's type.
        if left.is_float_value() && right.is_int_value() {
            let float_ty = left.into_float_value().get_type();
            let int_val = right.into_int_value();
            right = self
                .builder
                .build_signed_int_to_float(int_val, float_ty, "to_float")
                .unwrap()
                .into();
        } else if left.is_int_value() && right.is_float_value() {
            let float_ty = right.into_float_value().get_type();
            let int_val = left.into_int_value();
            left = self
                .builder
                .build_signed_int_to_float(int_val, float_ty, "to_float")
                .unwrap()
                .into();
        } else if left.is_int_value() && right.is_int_value() {
            // Align integer widths (sign-extend the narrower operand).
            let lw = left.into_int_value().get_type().get_bit_width();
            let rw = right.into_int_value().get_type().get_bit_width();
            if lw < rw {
                let target = right.into_int_value().get_type();
                left = self
                    .builder
                    .build_int_s_extend(left.into_int_value(), target, "widen")
                    .unwrap()
                    .into();
            } else if rw < lw {
                let target = left.into_int_value().get_type();
                right = self
                    .builder
                    .build_int_s_extend(right.into_int_value(), target, "widen")
                    .unwrap()
                    .into();
            }
        }

        let value = match expr.operator {
            BinOp::Add => {
                if left.is_int_value() {
                    self.builder
                        .build_int_add(left.into_int_value(), right.into_int_value(), "add")
                        .unwrap()
                        .into()
                } else {
                    self.builder
                        .build_float_add(left.into_float_value(), right.into_float_value(), "fadd")
                        .unwrap()
                        .into()
                }
            }
            BinOp::Sub => {
                if left.is_int_value() {
                    self.builder
                        .build_int_sub(left.into_int_value(), right.into_int_value(), "sub")
                        .unwrap()
                        .into()
                } else {
                    self.builder
                        .build_float_sub(left.into_float_value(), right.into_float_value(), "fsub")
                        .unwrap()
                        .into()
                }
            }
            BinOp::Mul => {
                if left.is_int_value() {
                    self.builder
                        .build_int_mul(left.into_int_value(), right.into_int_value(), "mul")
                        .unwrap()
                        .into()
                } else {
                    self.builder
                        .build_float_mul(left.into_float_value(), right.into_float_value(), "fmul")
                        .unwrap()
                        .into()
                }
            }
            BinOp::Div => {
                if left.is_int_value() {
                    self.builder
                        .build_int_signed_div(left.into_int_value(), right.into_int_value(), "div")
                        .unwrap()
                        .into()
                } else {
                    self.builder
                        .build_float_div(left.into_float_value(), right.into_float_value(), "fdiv")
                        .unwrap()
                        .into()
                }
            }
            BinOp::Mod => {
                if left.is_int_value() {
                    self.builder
                        .build_int_signed_rem(left.into_int_value(), right.into_int_value(), "mod")
                        .unwrap()
                        .into()
                } else {
                    return Err(HuziError::new_global(
                        "Operator '%' requires integer operands",
                    ));
                }
            }
            BinOp::Eq => self.build_int_or_float_compare(
                inkwell::IntPredicate::EQ,
                inkwell::FloatPredicate::OEQ,
                &left,
                &right,
            ),
            BinOp::Neq => self.build_int_or_float_compare(
                inkwell::IntPredicate::NE,
                inkwell::FloatPredicate::ONE,
                &left,
                &right,
            ),
            BinOp::Lt => self.build_int_or_float_compare(
                inkwell::IntPredicate::SLT,
                inkwell::FloatPredicate::OLT,
                &left,
                &right,
            ),
            BinOp::Le => self.build_int_or_float_compare(
                inkwell::IntPredicate::SLE,
                inkwell::FloatPredicate::OLE,
                &left,
                &right,
            ),
            BinOp::Gt => self.build_int_or_float_compare(
                inkwell::IntPredicate::SGT,
                inkwell::FloatPredicate::OGT,
                &left,
                &right,
            ),
            BinOp::Ge => self.build_int_or_float_compare(
                inkwell::IntPredicate::SGE,
                inkwell::FloatPredicate::OGE,
                &left,
                &right,
            ),
            BinOp::And | BinOp::Or => unreachable!("short-circuit handled above"),
        };

        Ok(value)
    }

    fn build_int_or_float_compare(
        &self,
        int_pred: inkwell::IntPredicate,
        float_pred: inkwell::FloatPredicate,
        left: &inkwell::values::BasicValueEnum<'ctx>,
        right: &inkwell::values::BasicValueEnum<'ctx>,
    ) -> inkwell::values::BasicValueEnum<'ctx> {
        if left.is_int_value() {
            self.builder
                .build_int_compare(int_pred, left.into_int_value(), right.into_int_value(), "cmp")
                .unwrap()
                .into()
        } else {
            self.builder
                .build_float_compare(
                    float_pred,
                    left.into_float_value(),
                    right.into_float_value(),
                    "cmp",
                )
                .unwrap()
                .into()
        }
    }

    fn compile_short_circuit(
        &mut self,
        left: &Expr,
        right: &Expr,
        is_and: bool,
    ) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        let function = self.current_function()?;

        let rhs_block = self.context.append_basic_block(function, "sc_rhs");
        let short_block = self.context.append_basic_block(function, "sc_short");
        let end_block = self.context.append_basic_block(function, "sc_end");

        let result_ptr = self.build_alloca(self.context.bool_type().into(), "sc_result")?;

        let lhs_value = self.compile_expr(left)?;
        let lhs = self.to_i1(lhs_value)?;
        if is_and {
            self.builder
                .build_conditional_branch(lhs, rhs_block, short_block)
                .unwrap();
        } else {
            self.builder
                .build_conditional_branch(lhs, short_block, rhs_block)
                .unwrap();
        }

        // Short-circuit branch: result is false (for &&) or true (for ||).
        self.builder.position_at_end(short_block);
        let short_val = self.context.bool_type().const_int(!is_and as u64, false);
        self.builder.build_store(result_ptr, short_val).unwrap();
        self.builder
            .build_unconditional_branch(end_block)
            .unwrap();

        // Evaluate the right operand only when needed.
        self.builder.position_at_end(rhs_block);
        let rhs_value = self.compile_expr(right)?;
        let rhs = self.to_i1(rhs_value)?;
        self.builder.build_store(result_ptr, rhs).unwrap();
        self.builder
            .build_unconditional_branch(end_block)
            .unwrap();

        self.builder.position_at_end(end_block);
        let result = self
            .builder
            .build_load(self.context.bool_type(), result_ptr, "sc_load")
            .unwrap();

        Ok(result)
    }

    fn compile_unary(
        &mut self,
        expr: &UnaryExpr,
    ) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        let operand = self.compile_expr(&expr.operand)?;

        let value = match expr.operator {
            UnOp::Neg => {
                if operand.is_int_value() {
                    self.builder
                        .build_int_neg(operand.into_int_value(), "neg")
                        .unwrap()
                        .into()
                } else {
                    self.builder
                        .build_float_neg(operand.into_float_value(), "fneg")
                        .unwrap()
                        .into()
                }
            }
            UnOp::Not => {
                let cond = self.to_i1(operand)?;
                self.builder.build_not(cond, "not").unwrap().into()
            }
        };

        Ok(value)
    }

    fn compile_call(
        &mut self,
        expr: &CallExpr,
    ) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        let callee_name = match &*expr.callee {
            Expr::Ident(name) => name.clone(),
            _ => return Err(HuziError::new_global("Expected function name")),
        };

        // Built-in functions
        match callee_name.as_str() {
            "print" => return self.compile_print(&expr.arguments),
            "read_line" => return self.compile_read_line(),
            "read_int" => return self.compile_read_int(),
            "read_float" => return self.compile_read_float(),
            "len" => return self.compile_len(&expr.arguments),
            "abs" => return self.compile_abs(&expr.arguments),
            "sqrt" => return self.compile_sqrt(&expr.arguments),
            "pow" => return self.compile_pow(&expr.arguments),
            "sin" => return self.compile_sin(&expr.arguments),
            "cos" => return self.compile_cos(&expr.arguments),
            "concat" => return self.compile_concat(&expr.arguments),
            "to_string" => return self.compile_to_string(&expr.arguments),
            _ => {}
        }

        let (function, param_types) = self
            .functions
            .get(&callee_name)
            .cloned()
            .ok_or_else(|| HuziError::new_global(format!("Unknown function: {}", callee_name)))?;

        if expr.arguments.len() != param_types.len() {
            return Err(HuziError::new_global(format!(
                "Function '{}' expects {} argument(s), got {}",
                callee_name,
                param_types.len(),
                expr.arguments.len()
            )));
        }

        let mut args: Vec<inkwell::values::BasicMetadataValueEnum> = Vec::new();
        for (arg_expr, param_type) in expr.arguments.iter().zip(param_types.iter()) {
            let value = self.compile_expr(arg_expr)?;
            let value = self.coerce_value(*param_type, value)?;
            args.push(value.into());
        }

        let call = self.builder.build_call(function, &args, "call").unwrap();

        Ok(call.try_as_basic_value().unwrap_left())
    }

    fn compile_print(
        &mut self,
        arguments: &[Expr],
    ) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        let printf_fn = self.module.get_function("printf").unwrap();

        if arguments.is_empty() {
            let empty_str = unsafe { self.builder.build_global_string("", "empty_str").unwrap() };
            let call = self
                .builder
                .build_call(
                    printf_fn,
                    &[empty_str.as_pointer_value().into()],
                    "print_empty",
                )
                .unwrap();
            return Ok(call.try_as_basic_value().unwrap_left());
        }

        let mut format_string = String::new();
        let mut args: Vec<inkwell::values::BasicMetadataValueEnum> = Vec::new();

        for arg in arguments.iter() {
            let value = self.compile_expr(arg)?;

            match value {
                inkwell::values::BasicValueEnum::IntValue(iv)
                    if iv.get_type().get_bit_width() == 1 =>
                {
                    // Booleans print as true/false.
                    let s = self.build_bool_str(iv)?;
                    format_string.push_str("%s");
                    args.push(s.into());
                }
                inkwell::values::BasicValueEnum::IntValue(iv) => {
                    match iv.get_type().get_bit_width() {
                        8 => {
                            // Chars are printed as characters.
                            format_string.push_str("%c");
                            let c = self
                                .builder
                                .build_int_z_extend(iv, self.context.i32_type(), "char_promote")
                                .unwrap();
                            args.push(c.into());
                        }
                        64 => {
                            format_string.push_str("%ld");
                            args.push(iv.into());
                        }
                        _ => {
                            format_string.push_str("%d");
                            args.push(iv.into());
                        }
                    }
                }
                inkwell::values::BasicValueEnum::FloatValue(fv) => {
                    // varargs promote floats to double
                    let f64_val = if fv.get_type() == self.context.f64_type() {
                        fv
                    } else {
                        self.builder
                            .build_float_ext(fv, self.context.f64_type(), "f_promote")
                            .unwrap()
                    };
                    if fv.get_type() == self.context.f32_type() {
                        format_string.push_str("%g");
                    } else {
                        format_string.push_str("%f");
                    }
                    args.push(f64_val.into());
                }
                inkwell::values::BasicValueEnum::PointerValue(pv) => {
                    format_string.push_str("%s");
                    args.push(pv.into());
                }
                _ => {
                    return Err(HuziError::new_global(
                        "print() does not support this value type",
                    ))
                }
            }
        }

        format_string.push('\n');

        let format_ptr = unsafe {
            self.builder
                .build_global_string(&format_string, "format_str")
                .unwrap()
        };

        let mut call_args = vec![format_ptr.as_pointer_value().into()];
        call_args.extend(args);

        let call = self
            .builder
            .build_call(printf_fn, &call_args, "print_call")
            .unwrap();

        Ok(call.try_as_basic_value().unwrap_left())
    }

    /// Build a global "true"/"false" string selected by the given i1 condition.
    fn build_bool_str(
        &mut self,
        cond: inkwell::values::IntValue<'ctx>,
    ) -> Result<PointerValue<'ctx>> {
        let true_ptr = match self.module.get_global("huzi_str_true") {
            Some(g) => g.as_pointer_value(),
            None => unsafe { self.builder.build_global_string("true", "huzi_str_true").unwrap() }
                .as_pointer_value(),
        };
        let false_ptr = match self.module.get_global("huzi_str_false") {
            Some(g) => g.as_pointer_value(),
            None => unsafe {
                self.builder
                    .build_global_string("false", "huzi_str_false")
                    .unwrap()
            }
            .as_pointer_value(),
        };

        let selected = self
            .builder
            .build_select(cond, true_ptr, false_ptr, "bool_str")
            .unwrap();

        Ok(selected.into_pointer_value())
    }

    fn compile_assign(
        &mut self,
        expr: &AssignExpr,
    ) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        let value = self.compile_expr(&expr.value)?;

        match &*expr.target {
            Expr::Ident(name) => {
                let slot = self
                    .scope_lookup(name)
                    .ok_or_else(|| HuziError::new_global(format!("Unknown variable: {}", name)))?;

                if !slot.mutable {
                    return Err(HuziError::new_global(format!(
                        "Cannot assign to immutable variable '{}'; declare it with `let mut`",
                        name
                    )));
                }

                let value = self.coerce_value(slot.ty, value)?;
                self.builder.build_store(slot.ptr, value).unwrap();
                Ok(value)
            }
            Expr::ArrayIndex(idx_expr) => {
                let array_ptr = self.compile_expr(&idx_expr.array)?;
                let array_ptr = if array_ptr.is_pointer_value() {
                    array_ptr.into_pointer_value()
                } else {
                    return Err(HuziError::new_global("Indexed value is not an array"));
                };

                let elem_type = self.resolve_elem_type(&idx_expr.array, Some(value.get_type()))?;

                let index_val = self.compile_expr(&idx_expr.index)?;
                let index_i32 = self.coerce_index(index_val)?;

                let value = self.coerce_value(elem_type, value)?;

                let elem_ptr = unsafe {
                    self.builder
                        .build_gep(elem_type, array_ptr, &[index_i32], "elem_ptr")
                        .unwrap()
                };
                self.builder.build_store(elem_ptr, value).unwrap();
                Ok(value)
            }
            _ => Err(HuziError::new_global("Invalid assignment target")),
        }
    }

    /// Determine the element type for an indexed expression.
    fn resolve_elem_type(
        &self,
        array_expr: &Expr,
        fallback: Option<inkwell::types::BasicTypeEnum<'ctx>>,
    ) -> Result<inkwell::types::BasicTypeEnum<'ctx>> {
        if let Expr::Ident(name) = array_expr {
            if let Some(slot) = self.scope_lookup(name) {
                if let Some(elem) = slot.elem {
                    return Ok(elem);
                }
            }
        }
        if let Expr::Literal(Literal::String(_)) = array_expr {
            return Ok(self.context.i8_type().into());
        }
        fallback.ok_or_else(|| HuziError::new_global("Cannot determine array element type"))
    }

    fn coerce_index(
        &self,
        index_val: inkwell::values::BasicValueEnum<'ctx>,
    ) -> Result<inkwell::values::IntValue<'ctx>> {
        if !index_val.is_int_value() {
            return Err(HuziError::new_global("Array index must be an integer"));
        }
        let int_val = index_val.into_int_value();
        if int_val.get_type().get_bit_width() == 32 {
            Ok(int_val)
        } else if int_val.get_type().get_bit_width() < 32 {
            Ok(self
                .builder
                .build_int_s_extend(int_val, self.context.i32_type(), "index_i32")
                .unwrap())
        } else {
            Ok(self
                .builder
                .build_int_truncate(int_val, self.context.i32_type(), "index_i32")
                .unwrap())
        }
    }

    fn type_to_llvm(&self, ty: &Type) -> Result<inkwell::types::BasicTypeEnum<'ctx>> {
        match ty {
            Type::I32 | Type::U32 => Ok(self.context.i32_type().into()),
            Type::I64 | Type::U64 => Ok(self.context.i64_type().into()),
            Type::F32 => Ok(self.context.f32_type().into()),
            Type::F64 => Ok(self.context.f64_type().into()),
            Type::Bool => Ok(self.context.bool_type().into()),
            Type::Char => Ok(self.context.i8_type().into()),
            Type::Str => Ok(self.context.ptr_type(AddressSpace::default()).into()),
            Type::Unit => Ok(self.context.i32_type().into()),
            // Arrays decay to pointers (LLVM opaque pointers make these
            // equivalent); element types are tracked in VarSlot.
            Type::Array(_, _) => Ok(self.context.ptr_type(AddressSpace::default()).into()),
            Type::Named(name) => match name.as_str() {
                "i32" | "u32" => Ok(self.context.i32_type().into()),
                "i64" | "u64" => Ok(self.context.i64_type().into()),
                "f32" => Ok(self.context.f32_type().into()),
                "f64" => Ok(self.context.f64_type().into()),
                "bool" => Ok(self.context.bool_type().into()),
                "char" => Ok(self.context.i8_type().into()),
                "str" => Ok(self.context.ptr_type(AddressSpace::default()).into()),
                other => Err(HuziError::new_global(format!("Unsupported type: {}", other))),
            },
        }
    }

    /// Convert a value to a truthy i1.
    fn to_i1(&self, value: inkwell::values::BasicValueEnum<'ctx>) -> Result<inkwell::values::IntValue<'ctx>> {
        match value {
            inkwell::values::BasicValueEnum::IntValue(iv)
                if iv.get_type().get_bit_width() == 1 =>
            {
                Ok(iv)
            }
            inkwell::values::BasicValueEnum::IntValue(iv) => {
                let zero = iv.get_type().const_int(0, false);
                Ok(self
                    .builder
                    .build_int_compare(inkwell::IntPredicate::NE, iv, zero, "to_bool")
                    .unwrap())
            }
            inkwell::values::BasicValueEnum::PointerValue(pv) => Ok(self
                .builder
                .build_is_not_null(pv, "to_bool")
                .unwrap()),
            _ => Err(HuziError::new_global(
                "Value cannot be used as a condition",
            )),
        }
    }

    /// Convert a value to the target type when it is a lossless/expected
    /// numeric coercion; otherwise report a type mismatch.
    fn coerce_value(
        &self,
        target: inkwell::types::BasicTypeEnum<'ctx>,
        value: inkwell::values::BasicValueEnum<'ctx>,
    ) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        if value.get_type() == target {
            return Ok(value);
        }

        match (target, value) {
            (inkwell::types::BasicTypeEnum::IntType(it), inkwell::values::BasicValueEnum::IntValue(iv)) => {
                let tw = it.get_bit_width();
                let sw = iv.get_type().get_bit_width();
                if tw > sw {
                    Ok(self
                        .builder
                        .build_int_s_extend(iv, it, "coerce")
                        .unwrap()
                        .into())
                } else if tw < sw {
                    Ok(self
                        .builder
                        .build_int_truncate(iv, it, "coerce")
                        .unwrap()
                        .into())
                } else {
                    Ok(iv.into())
                }
            }
            (inkwell::types::BasicTypeEnum::FloatType(ft), inkwell::values::BasicValueEnum::IntValue(iv)) => {
                Ok(self
                    .builder
                    .build_signed_int_to_float(iv, ft, "coerce")
                    .unwrap()
                    .into())
            }
            (inkwell::types::BasicTypeEnum::IntType(it), inkwell::values::BasicValueEnum::FloatValue(fv)) => {
                Ok(self
                    .builder
                    .build_float_to_signed_int(fv, it, "coerce")
                    .unwrap()
                    .into())
            }
            (inkwell::types::BasicTypeEnum::FloatType(ft), inkwell::values::BasicValueEnum::FloatValue(fv)) => {
                if fv.get_type() == ft {
                    Ok(fv.into())
                } else {
                    Ok(self.builder.build_float_cast(fv, ft, "coerce").unwrap().into())
                }
            }
            (inkwell::types::BasicTypeEnum::IntType(it), inkwell::values::BasicValueEnum::PointerValue(pv))
                if it.get_bit_width() == 64 =>
            {
                Ok(self.builder.build_ptr_to_int(pv, it, "coerce").unwrap().into())
            }
            _ => Err(HuziError::new_global(format!(
                "Type mismatch: expected {}, got {}",
                target,
                value.get_type()
            ))),
        }
    }

    fn build_alloca(
        &self,
        ty: inkwell::types::BasicTypeEnum<'ctx>,
        name: &str,
    ) -> Result<PointerValue<'ctx>> {
        let function = self.current_function()?;
        let entry = function
            .get_first_basic_block()
            .ok_or_else(|| HuziError::new_global("Function has no entry block"))?;
        let builder = self.context.create_builder();
        // Insert before the first instruction so allocas always land at the
        // top of the entry block, never after a terminator.
        if let Some(first) = entry.get_first_instruction() {
            builder.position_before(&first);
        } else {
            builder.position_at_end(entry);
        }
        builder
            .build_alloca(ty, name)
            .map_err(|_| HuziError::new_global("Failed to build alloca"))
    }

    // ==================== Standard Library Functions ====================

    fn compile_read_line(&mut self) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        let malloc_fn = self.module.get_function("malloc").unwrap();
        let getchar_fn = self.module.get_function("getchar").unwrap();

        // Allocate buffer (256 bytes)
        let buffer_size = self.context.i32_type().const_int(256, false);
        let buffer = self
            .builder
            .build_call(malloc_fn, &[buffer_size.into()], "buffer")
            .unwrap()
            .try_as_basic_value()
            .unwrap_left()
            .into_pointer_value();

        let i32_type = self.context.i32_type();
        let idx_ptr = self.build_alloca(i32_type.into(), "read_idx")?;
        self.builder
            .build_store(idx_ptr, i32_type.const_int(0, false))
            .unwrap();

        let function = self.current_function()?;
        let loop_block = self.context.append_basic_block(function, "read_loop");
        let store_block = self.context.append_basic_block(function, "read_store");
        let done_block = self.context.append_basic_block(function, "read_done");

        self.builder
            .build_unconditional_branch(loop_block)
            .unwrap();

        // Read one char per iteration until '\n', EOF, or buffer full.
        self.builder.position_at_end(loop_block);
        let c = self
            .builder
            .build_call(getchar_fn, &[], "ch")
            .unwrap()
            .try_as_basic_value()
            .unwrap_left()
            .into_int_value();

        let idx = self
            .builder
            .build_load(i32_type, idx_ptr, "idx")
            .unwrap()
            .into_int_value();

        let has_space = self
            .builder
            .build_int_compare(inkwell::IntPredicate::SLT, idx, i32_type.const_int(255, false), "has_space")
            .unwrap();
        let not_nl = self
            .builder
            .build_int_compare(inkwell::IntPredicate::NE, c, i32_type.const_int('\n' as u64, false), "not_nl")
            .unwrap();
        let not_eof = self
            .builder
            .build_int_compare(inkwell::IntPredicate::NE, c, i32_type.const_int(-1i64 as u64, true), "not_eof")
            .unwrap();
        let cont = self.builder.build_and(has_space, not_nl, "cont").unwrap();
        let cont = self.builder.build_and(cont, not_eof, "cont2").unwrap();

        self.builder
            .build_conditional_branch(cont, store_block, done_block)
            .unwrap();

        self.builder.position_at_end(store_block);
        let c8 = self
            .builder
            .build_int_truncate(c, self.context.i8_type(), "ch_i8")
            .unwrap();
        let ch_ptr = unsafe {
            self.builder
                .build_gep(self.context.i8_type(), buffer, &[idx], "ch_ptr")
                .unwrap()
        };
        self.builder.build_store(ch_ptr, c8).unwrap();
        let idx_next = self
            .builder
            .build_int_add(idx, i32_type.const_int(1, false), "idx_next")
            .unwrap();
        self.builder.build_store(idx_ptr, idx_next).unwrap();
        self.builder
            .build_unconditional_branch(loop_block)
            .unwrap();

        // Null-terminate and continue in the done block.
        self.builder.position_at_end(done_block);
        let term_ptr = unsafe {
            self.builder
                .build_gep(self.context.i8_type(), buffer, &[idx], "term_ptr")
                .unwrap()
        };
        self.builder
            .build_store(term_ptr, self.context.i8_type().const_int(0, false))
            .unwrap();

        Ok(buffer.into())
    }

    fn compile_read_int(&mut self) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        let scanf_fn = self.module.get_function("scanf").unwrap();

        // Format string for %d
        let format_str = unsafe {
            self.builder
                .build_global_string("%d", "scanf_format_int")
                .unwrap()
        };

        // Allocate space for int
        let int_ptr = self.build_alloca(self.context.i32_type().into(), "int_input")?;

        self.builder
            .build_call(
                scanf_fn,
                &[
                    format_str.as_pointer_value().into(),
                    int_ptr.into(),
                ],
                "scanf_int",
            )
            .unwrap();

        let value = self
            .builder
            .build_load(self.context.i32_type(), int_ptr, "int_value")
            .unwrap();

        Ok(value)
    }

    fn compile_read_float(&mut self) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        let scanf_fn = self.module.get_function("scanf").unwrap();

        // Format string for %lf
        let format_str = unsafe {
            self.builder
                .build_global_string("%lf", "scanf_format_float")
                .unwrap()
        };

        // Allocate space for double
        let float_ptr = self.build_alloca(self.context.f64_type().into(), "float_input")?;

        self.builder
            .build_call(
                scanf_fn,
                &[
                    format_str.as_pointer_value().into(),
                    float_ptr.into(),
                ],
                "scanf_float",
            )
            .unwrap();

        let value = self
            .builder
            .build_load(self.context.f64_type(), float_ptr, "float_value")
            .unwrap();

        Ok(value)
    }

    fn compile_len(&mut self, arguments: &[Expr]) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        if arguments.len() != 1 {
            return Err(HuziError::new_global("len() requires exactly 1 argument"));
        }

        // len(arr) on an array variable returns the tracked array length;
        // strings use strlen.
        if let Expr::Ident(name) = &arguments[0] {
            if let Some(slot) = self.scope_lookup(name) {
                if let Some(len) = slot.array_len {
                    return Ok(self.context.i32_type().const_int(len as u64, false).into());
                }
            }
        }

        let arg = self.compile_expr(&arguments[0])?;
        let arg = if arg.is_pointer_value() {
            arg.into_pointer_value()
        } else {
            return Err(HuziError::new_global("len() requires a string or array argument"));
        };

        let strlen_fn = self.module.get_function("strlen").unwrap();
        let len = self
            .builder
            .build_call(strlen_fn, &[arg.into()], "str_len")
            .unwrap()
            .try_as_basic_value()
            .unwrap_left();

        Ok(len)
    }

    fn compile_abs(&mut self, arguments: &[Expr]) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        if arguments.len() != 1 {
            return Err(HuziError::new_global("abs() requires exactly 1 argument"));
        }

        let arg = self.compile_expr(&arguments[0])?;

        if arg.is_int_value() {
            let int_val = arg.into_int_value();
            let zero = int_val.get_type().const_int(0, false);
            let is_neg = self
                .builder
                .build_int_compare(inkwell::IntPredicate::SLT, int_val, zero, "abs_neg")
                .unwrap();
            let negated = self.builder.build_int_neg(int_val, "abs_negated").unwrap();
            let result = self
                .builder
                .build_select(is_neg, negated, int_val, "abs")
                .unwrap();
            Ok(result)
        } else if arg.is_float_value() {
            let float_val = arg.into_float_value();
            let fabs_fn = self.module.get_function("fabs").unwrap();
            let arg_f64 = if float_val.get_type() == self.context.f64_type() {
                float_val
            } else {
                self.builder
                    .build_float_cast(float_val, self.context.f64_type(), "to_f64")
                    .unwrap()
            };
            let result = self
                .builder
                .build_call(fabs_fn, &[arg_f64.into()], "abs_result")
                .unwrap()
                .try_as_basic_value()
                .unwrap_left();
            Ok(result)
        } else {
            Err(HuziError::new_global("abs() requires a numeric argument"))
        }
    }

    fn compile_sqrt(&mut self, arguments: &[Expr]) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        if arguments.len() != 1 {
            return Err(HuziError::new_global("sqrt() requires exactly 1 argument"));
        }

        let sqrt_fn = self.module.get_function("sqrt").unwrap();
        let arg = self.compile_expr(&arguments[0])?;
        let arg_f64 = self.to_f64(arg, "sqrt()")?;

        let result = self
            .builder
            .build_call(sqrt_fn, &[arg_f64.into()], "sqrt_result")
            .unwrap()
            .try_as_basic_value()
            .unwrap_left();

        Ok(result)
    }

    fn compile_pow(&mut self, arguments: &[Expr]) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        if arguments.len() != 2 {
            return Err(HuziError::new_global("pow() requires exactly 2 arguments"));
        }

        let pow_fn = self.module.get_function("pow").unwrap();
        let base = self.compile_expr(&arguments[0])?;
        let exp = self.compile_expr(&arguments[1])?;
        let base_f64 = self.to_f64(base, "pow()")?;
        let exp_f64 = self.to_f64(exp, "pow()")?;

        let result = self
            .builder
            .build_call(pow_fn, &[base_f64.into(), exp_f64.into()], "pow_result")
            .unwrap()
            .try_as_basic_value()
            .unwrap_left();

        Ok(result)
    }

    fn compile_sin(&mut self, arguments: &[Expr]) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        if arguments.len() != 1 {
            return Err(HuziError::new_global("sin() requires exactly 1 argument"));
        }

        let sin_fn = self.module.get_function("sin").unwrap();
        let arg = self.compile_expr(&arguments[0])?;
        let arg_f64 = self.to_f64(arg, "sin()")?;

        let result = self
            .builder
            .build_call(sin_fn, &[arg_f64.into()], "sin_result")
            .unwrap()
            .try_as_basic_value()
            .unwrap_left();

        Ok(result)
    }

    fn compile_cos(&mut self, arguments: &[Expr]) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        if arguments.len() != 1 {
            return Err(HuziError::new_global("cos() requires exactly 1 argument"));
        }

        let cos_fn = self.module.get_function("cos").unwrap();
        let arg = self.compile_expr(&arguments[0])?;
        let arg_f64 = self.to_f64(arg, "cos()")?;

        let result = self
            .builder
            .build_call(cos_fn, &[arg_f64.into()], "cos_result")
            .unwrap()
            .try_as_basic_value()
            .unwrap_left();

        Ok(result)
    }

    /// Convert any numeric value to f64 for math builtins.
    fn to_f64(
        &self,
        arg: inkwell::values::BasicValueEnum<'ctx>,
        fn_name: &str,
    ) -> Result<inkwell::values::FloatValue<'ctx>> {
        match arg {
            inkwell::values::BasicValueEnum::IntValue(iv) => Ok(self
                .builder
                .build_signed_int_to_float(iv, self.context.f64_type(), "to_f64")
                .unwrap()),
            inkwell::values::BasicValueEnum::FloatValue(fv) => {
                if fv.get_type() == self.context.f64_type() {
                    Ok(fv)
                } else {
                    Ok(self
                        .builder
                        .build_float_cast(fv, self.context.f64_type(), "to_f64")
                        .unwrap())
                }
            }
            _ => Err(HuziError::new_global(format!(
                "{} requires a numeric argument",
                fn_name
            ))),
        }
    }

    fn compile_concat(&mut self, arguments: &[Expr]) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        if arguments.len() < 2 {
            return Err(HuziError::new_global("concat() requires at least 2 arguments"));
        }

        let malloc_fn = self.module.get_function("malloc").unwrap();
        let strcpy_fn = self.module.get_function("strcpy").unwrap();
        let strlen_fn = self.module.get_function("strlen").unwrap();

        let mut arg_ptrs = Vec::with_capacity(arguments.len());
        let mut arg_lens = Vec::with_capacity(arguments.len());
        for a in arguments {
            let v = self.compile_expr(a)?;
            let ptr = if v.is_pointer_value() {
                v.into_pointer_value()
            } else {
                return Err(HuziError::new_global("concat() requires string arguments"));
            };
            let len = self
                .builder
                .build_call(strlen_fn, &[ptr.into()], "len")
                .unwrap()
                .try_as_basic_value()
                .unwrap_left()
                .into_int_value();
            arg_ptrs.push(ptr);
            arg_lens.push(len);
        }

        // Allocate len(args...) + 1 for the null terminator.
        let i32_type = self.context.i32_type();
        let mut total_len = i32_type.const_int(0, false);
        for len in &arg_lens {
            total_len = self
                .builder
                .build_int_add(total_len, *len, "total_len")
                .unwrap();
        }
        let alloc_size = self
            .builder
            .build_int_add(total_len, i32_type.const_int(1, false), "alloc_size")
            .unwrap();
        let buffer = self
            .builder
            .build_call(malloc_fn, &[alloc_size.into()], "concat_buffer")
            .unwrap()
            .try_as_basic_value()
            .unwrap_left()
            .into_pointer_value();

        // Copy the first string, then append each remaining one.
        self.builder
            .build_call(strcpy_fn, &[buffer.into(), arg_ptrs[0].into()], "copy")
            .unwrap();
        let mut offset = arg_lens[0];
        for (ptr, len) in arg_ptrs.iter().zip(arg_lens.iter()).skip(1) {
            let dest = unsafe {
                self.builder
                    .build_gep(self.context.i8_type(), buffer, &[offset], "concat_dest")
                    .unwrap()
            };
            self.builder
                .build_call(strcpy_fn, &[dest.into(), (*ptr).into()], "copy")
                .unwrap();
            offset = self
                .builder
                .build_int_add(offset, *len, "offset")
                .unwrap();
        }

        Ok(buffer.into())
    }

    fn compile_to_string(&mut self, arguments: &[Expr]) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        if arguments.len() != 1 {
            return Err(HuziError::new_global("to_string() requires exactly 1 argument"));
        }

        let malloc_fn = self.module.get_function("malloc").unwrap();
        let sprintf_fn = self.module.get_function("sprintf").unwrap();

        let arg = self.compile_expr(&arguments[0])?;

        // Determine format string based on type
        let format_str = match arg {
            inkwell::values::BasicValueEnum::IntValue(iv)
                if iv.get_type().get_bit_width() == 1 =>
            {
                let s = self.build_bool_str(iv)?;
                let buffer_size = self.context.i32_type().const_int(8, false);
                let buffer = self
                    .builder
                    .build_call(malloc_fn, &[buffer_size.into()], "str_buffer")
                    .unwrap()
                    .try_as_basic_value()
                    .unwrap_left()
                    .into_pointer_value();
                self.builder
                    .build_call(
                        sprintf_fn,
                        &[buffer.into(), s.into()],
                        "sprintf",
                    )
                    .unwrap();
                return Ok(buffer.into());
            }
            inkwell::values::BasicValueEnum::IntValue(iv) => {
                if iv.get_type().get_bit_width() == 64 {
                    let fmt = unsafe { self.builder.build_global_string("%ld", "fmt_i64").unwrap() };
                    (fmt, inkwell::values::BasicValueEnum::IntValue(iv))
                } else if iv.get_type().get_bit_width() == 8 {
                    let fmt = unsafe { self.builder.build_global_string("%c", "fmt_c").unwrap() };
                    let promoted = self
                        .builder
                        .build_int_z_extend(iv, self.context.i32_type(), "char_promote")
                        .unwrap();
                    (fmt, inkwell::values::BasicValueEnum::IntValue(promoted))
                } else {
                    let fmt = unsafe { self.builder.build_global_string("%d", "fmt_i32").unwrap() };
                    (fmt, inkwell::values::BasicValueEnum::IntValue(iv))
                }
            }
            inkwell::values::BasicValueEnum::FloatValue(fv) => {
                // Promote to double for printf-style varargs.
                let f64_val = if fv.get_type() == self.context.f64_type() {
                    fv
                } else {
                    self.builder
                        .build_float_ext(fv, self.context.f64_type(), "f_promote")
                        .unwrap()
                };
                if fv.get_type() == self.context.f32_type() {
                    let fmt = unsafe { self.builder.build_global_string("%g", "fmt_f32").unwrap() };
                    (fmt, inkwell::values::BasicValueEnum::FloatValue(f64_val))
                } else {
                    let fmt = unsafe { self.builder.build_global_string("%f", "fmt_f64").unwrap() };
                    (fmt, inkwell::values::BasicValueEnum::FloatValue(f64_val))
                }
            }
            inkwell::values::BasicValueEnum::PointerValue(_) => {
                return Err(HuziError::new_global(
                    "to_string() requires a numeric argument",
                ))
            }
            _ => {
                return Err(HuziError::new_global(
                    "to_string() unsupported type",
                ))
            }
        };

        // Allocate buffer (large enough for any double formatting)
        let buffer_size = self.context.i32_type().const_int(320, false);
        let buffer = self
            .builder
            .build_call(malloc_fn, &[buffer_size.into()], "str_buffer")
            .unwrap()
            .try_as_basic_value()
            .unwrap_left()
            .into_pointer_value();

        // Call sprintf
        self.builder
            .build_call(
                sprintf_fn,
                &[buffer.into(), format_str.0.as_pointer_value().into(), format_str.1.into()],
                "sprintf",
            )
            .unwrap();

        Ok(buffer.into())
    }

    // ==================== Array Functions ====================

    fn compile_array_index(
        &mut self,
        expr: &huzi_ast::ArrayIndexExpr,
    ) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        let array_ptr = self.compile_expr(&expr.array)?;
        let array_ptr_val = if array_ptr.is_pointer_value() {
            array_ptr.into_pointer_value()
        } else {
            return Err(HuziError::new_global("Indexed value is not an array"));
        };

        let elem_type = self.resolve_elem_type(&expr.array, None)?;

        let index_val = self.compile_expr(&expr.index)?;
        let index_i32 = self.coerce_index(index_val)?;

        // Build GEP to get element pointer
        let elem_ptr = unsafe {
            self.builder
                .build_gep(elem_type, array_ptr_val, &[index_i32], "elem_ptr")
                .unwrap()
        };

        // Load the element value
        let loaded = self
            .builder
            .build_load(elem_type, elem_ptr, "load_elem")
            .unwrap();

        Ok(loaded)
    }

    fn compile_array_literal(
        &mut self,
        elements: &[Expr],
    ) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        if elements.is_empty() {
            return Err(HuziError::new_global("Empty array literal not supported"));
        }

        // Compile all elements
        let mut elem_values = Vec::new();
        for elem in elements {
            let val = self.compile_expr(elem)?;
            elem_values.push(val);
        }

        // Get element type from first element
        let elem_type = elem_values[0].get_type();

        // Create array type
        let array_type = elem_type.array_type(elements.len() as u32);

        // Allocate space for the array
        let array_ptr = self.build_alloca(array_type.into(), "array")?;

        // Store each element
        for (i, val) in elem_values.iter().enumerate() {
            let val = self.coerce_value(elem_type, *val)?;
            let index = self.context.i32_type().const_int(i as u64, false);
            let elem_ptr = unsafe {
                self.builder
                    .build_gep(elem_type, array_ptr, &[index], "elem_ptr")
                    .unwrap()
            };
            self.builder.build_store(elem_ptr, val).unwrap();
        }

        Ok(array_ptr.into())
    }

    fn compile_if_expr(
        &mut self,
        expr: &IfExpr,
    ) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        let cond_value = self.compile_expr(&expr.condition)?;
        let cond = self.to_i1(cond_value)?;

        let function = self.current_function()?;
        let then_block = self.context.append_basic_block(function, "ifex_then");
        let else_block = self.context.append_basic_block(function, "ifex_else");
        let merge_block = self.context.append_basic_block(function, "ifex_merge");

        self.builder
            .build_conditional_branch(cond, then_block, else_block)
            .unwrap();

        self.builder.position_at_end(then_block);
        let then_val = self.compile_block_value(&expr.then_branch)?;
        let result_type = then_val.get_type();
        let result_ptr = self.build_alloca(result_type, "ifex_val")?;
        self.builder.build_store(result_ptr, then_val).unwrap();
        self.builder
            .build_unconditional_branch(merge_block)
            .unwrap();

        self.builder.position_at_end(else_block);
        let else_val = self.compile_block_value(&expr.else_branch)?;
        let else_val = self.coerce_value(result_type, else_val)?;
        self.builder.build_store(result_ptr, else_val).unwrap();
        self.builder
            .build_unconditional_branch(merge_block)
            .unwrap();

        self.builder.position_at_end(merge_block);
        let result = self
            .builder
            .build_load(result_type, result_ptr, "ifex_load")
            .unwrap();

        Ok(result)
    }

    // ==================== Output ====================

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
