use huzi_ast::*;
use huzi_error::{HuziError, HuziResult};
use inkwell::{
    builder::Builder,
    context::Context,
    module::Module,
    types::BasicType,
    values::{BasicValue, FunctionValue, PointerValue},
    AddressSpace,
};
use std::collections::HashMap;

pub struct CodeGen<'ctx> {
    context: &'ctx Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
    variables: HashMap<String, (PointerValue<'ctx>, inkwell::types::BasicTypeEnum<'ctx>)>,
    functions: HashMap<String, FunctionValue<'ctx>>,
}

impl<'ctx> CodeGen<'ctx> {
    pub fn new(context: &'ctx Context, name: &str) -> Self {
        let module = context.create_module(name);
        let builder = context.create_builder();

        Self {
            context,
            module,
            builder,
            variables: HashMap::new(),
            functions: HashMap::new(),
        }
    }

    pub fn compile(&mut self, program: &Program) -> HuziResult<()> {
        self.prelude()?;

        let fn_stmts: Vec<_> = program
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

        for stmt in &program.statements {
            match stmt {
                Stmt::Fn(_) => {}
                _ => self.compile_stmt(stmt)?,
            }
        }

        Ok(())
    }

    fn compile_fn_signature(&mut self, stmt: &FnStmt) -> HuziResult<()> {
        let param_types: Vec<inkwell::types::BasicMetadataTypeEnum<'ctx>> = stmt
            .params
            .iter()
            .map(|p| self.type_to_llvm(&p.param_type).map(|t| t.into()))
            .collect::<HuziResult<Vec<_>>>()?;

        let fn_type = if let Some(ret_type) = &stmt.return_type {
            self.type_to_llvm(ret_type)?.fn_type(&param_types, false)
        } else {
            self.context.i32_type().fn_type(&param_types, false)
        };

        let function = self.module.add_function(&stmt.name, fn_type, None);
        self.functions.insert(stmt.name.clone(), function);

        Ok(())
    }

    fn prelude(&mut self) -> HuziResult<()> {
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

        // malloc for string allocation
        let malloc_fn = self.context.i32_type().fn_type(
            &[self.context.i32_type().into()],
            false,
        );
        self.module.add_function("malloc", malloc_fn, None);

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

    fn compile_stmt(&mut self, stmt: &Stmt) -> HuziResult<()> {
        match stmt {
            Stmt::Let(let_stmt) => self.compile_let(let_stmt),
            Stmt::Fn(fn_stmt) => self.compile_fn(fn_stmt),
            Stmt::Expr(expr_stmt) => {
                self.compile_expr(&expr_stmt.expr)?;
                Ok(())
            }
            Stmt::Return(return_stmt) => self.compile_return(return_stmt),
            Stmt::Block(block) => self.compile_block(block),
            Stmt::If(if_stmt) => self.compile_if(if_stmt),
            Stmt::For(for_stmt) => self.compile_for(for_stmt),
            Stmt::While(while_stmt) => self.compile_while(while_stmt),
        }
    }

    fn compile_let(&mut self, stmt: &LetStmt) -> HuziResult<()> {
        if let Some(value) = &stmt.value {
            let value = self.compile_expr(value)?;
            let var_type = value.get_type();
            let alloca = self.build_alloca(var_type, &stmt.name).unwrap();
            self.builder.build_store(alloca, value).unwrap();
            self.variables.insert(stmt.name.clone(), (alloca, var_type));
        }
        Ok(())
    }

    fn compile_fn(&mut self, stmt: &FnStmt) -> HuziResult<()> {
        let function = self.functions.get(&stmt.name).copied().unwrap();
        let entry = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry);

        self.variables.clear();

        for (i, param) in stmt.params.iter().enumerate() {
            let arg = function.get_nth_param(i as u32).unwrap();
            let arg_type = arg.get_type();
            let alloca = self.build_alloca(arg_type, &param.name).unwrap();
            self.builder.build_store(alloca, arg).unwrap();
            self.variables
                .insert(param.name.clone(), (alloca, arg_type));
        }

        for s in &stmt.body.statements {
            self.compile_stmt(s)?;
        }

        if !self.builder.get_insert_block().is_none() {
            let block = self.builder.get_insert_block().unwrap();
            if block.get_terminator().is_none() {
                self.builder
                    .build_return(Some(&self.context.i32_type().const_int(0, false)))
                    .unwrap();
            }
        }

        self.functions.insert(stmt.name.clone(), function);

        Ok(())
    }

    fn compile_return(&mut self, stmt: &ReturnStmt) -> HuziResult<()> {
        if let Some(value) = &stmt.value {
            let value = self.compile_expr(value)?;
            self.builder.build_return(Some(&value)).unwrap();
        } else {
            self.builder
                .build_return(Some(&self.context.i32_type().const_int(0, false)))
                .unwrap();
        }
        Ok(())
    }

    fn compile_block(&mut self, block: &Block) -> HuziResult<()> {
        for stmt in &block.statements {
            self.compile_stmt(stmt)?;
        }
        Ok(())
    }

    fn compile_if(&mut self, stmt: &IfStmt) -> HuziResult<()> {
        let condition = self.compile_expr(&stmt.condition)?;

        let condition = match condition {
            inkwell::values::BasicValueEnum::IntValue(iv) => {
                if iv.get_type().get_bit_width() == 1 {
                    iv
                } else {
                    self.builder
                        .build_int_compare(
                            inkwell::IntPredicate::NE,
                            iv,
                            self.context.i32_type().const_int(0, false),
                            "if_cond",
                        )
                        .unwrap()
                }
            }
            _ => condition.into_int_value(),
        };

        let function = self
            .builder
            .get_insert_block()
            .unwrap()
            .get_parent()
            .unwrap();

        let then_block = self.context.append_basic_block(function, "then");
        let else_block = self.context.append_basic_block(function, "else");
        let merge_block = self.context.append_basic_block(function, "merge");

        self.builder
            .build_conditional_branch(condition, then_block, else_block)
            .unwrap();

        self.builder.position_at_end(then_block);
        self.compile_block(&stmt.then_branch)?;
        self.builder
            .build_unconditional_branch(merge_block)
            .unwrap();

        self.builder.position_at_end(else_block);
        if let Some(else_branch) = &stmt.else_branch {
            self.compile_block(else_branch)?;
        }
        self.builder
            .build_unconditional_branch(merge_block)
            .unwrap();

        self.builder.position_at_end(merge_block);

        Ok(())
    }

    fn compile_for(&mut self, stmt: &ForStmt) -> HuziResult<()> {
        let start = self.compile_expr(&stmt.start)?;
        let end = self.compile_expr(&stmt.end)?;

        let function = self
            .builder
            .get_insert_block()
            .unwrap()
            .get_parent()
            .unwrap();

        // Create blocks: loop, increment, after
        let loop_block = self.context.append_basic_block(function, "for_loop");
        let increment_block = self.context.append_basic_block(function, "for_increment");
        let after_block = self.context.append_basic_block(function, "for_after");

        // Allocate loop variable in current block (entry)
        let i_type = self.context.i32_type().into();
        let i_alloca = self.build_alloca(i_type, &stmt.var_name).unwrap();
        self.builder.build_store(i_alloca, start).unwrap();
        self.variables
            .insert(stmt.var_name.clone(), (i_alloca, i_type));

        // Branch to loop
        self.builder.build_unconditional_branch(loop_block).unwrap();

        // Loop block: check condition and execute body
        self.builder.position_at_end(loop_block);
        let i = self
            .builder
            .build_load(self.context.i32_type(), i_alloca, "i")
            .unwrap()
            .into_int_value();
        let condition = self
            .builder
            .build_int_compare(
                inkwell::IntPredicate::SLT,
                i,
                end.into_int_value(),
                "loop_cond",
            )
            .unwrap();

        // Conditionally branch to body or after
        let body_block = self.context.append_basic_block(function, "for_body");
        self.builder
            .build_conditional_branch(condition, body_block, after_block)
            .unwrap();

        // Body block: execute loop body
        self.builder.position_at_end(body_block);
        self.compile_block(&stmt.body)?;
        self.builder
            .build_unconditional_branch(increment_block)
            .unwrap();

        // Increment block: increment and jump back to loop
        self.builder.position_at_end(increment_block);
        let i = self
            .builder
            .build_load(self.context.i32_type(), i_alloca, "i")
            .unwrap()
            .into_int_value();
        let i_next = self
            .builder
            .build_int_add(i, self.context.i32_type().const_int(1, false), "i_next")
            .unwrap();
        self.builder.build_store(i_alloca, i_next).unwrap();
        self.builder.build_unconditional_branch(loop_block).unwrap();

        // After block: continue after loop
        self.builder.position_at_end(after_block);

        Ok(())
    }

    fn compile_while(&mut self, stmt: &WhileStmt) -> HuziResult<()> {
        let function = self
            .builder
            .get_insert_block()
            .unwrap()
            .get_parent()
            .unwrap();

        let loop_block = self.context.append_basic_block(function, "while_loop");
        let after_block = self.context.append_basic_block(function, "while_after");

        self.builder.build_unconditional_branch(loop_block).unwrap();

        self.builder.position_at_end(loop_block);

        let condition = self.compile_expr(&stmt.condition)?;
        let condition = self
            .builder
            .build_int_compare(
                inkwell::IntPredicate::NE,
                condition.into_int_value(),
                self.context.i32_type().const_int(0, false),
                "while_cond",
            )
            .unwrap();

        self.compile_block(&stmt.body)?;
        self.builder.build_unconditional_branch(loop_block).unwrap();

        self.builder
            .build_conditional_branch(condition, loop_block, after_block)
            .unwrap();

        self.builder.position_at_end(after_block);

        Ok(())
    }

    fn compile_expr(&mut self, expr: &Expr) -> HuziResult<inkwell::values::BasicValueEnum<'ctx>> {
        match expr {
            Expr::Literal(lit) => self.compile_literal(lit),
            Expr::Ident(name) => {
                if let Some((ptr, var_type)) = self.variables.get(name) {
                    let loaded = self.builder.build_load(*var_type, *ptr, "load").unwrap();
                    Ok(loaded.into())
                } else if let Some(func) = self.functions.get(name) {
                    Ok(func.as_global_value().as_basic_value_enum())
                } else {
                    Err(HuziError::new_global(format!("Unknown variable: {}", name)))
                }
            }
            Expr::Binary(bin_expr) => self.compile_binary(bin_expr),
            Expr::Unary(unary_expr) => self.compile_unary(unary_expr),
            Expr::Call(call_expr) => self.compile_call(call_expr),
            Expr::Assign(assign_expr) => self.compile_assign(assign_expr),
            Expr::ArrayIndex(idx_expr) => self.compile_array_index(idx_expr),
            Expr::ArrayLiteral(elements) => self.compile_array_literal(elements),
        }
    }

    fn compile_literal(&self, lit: &Literal) -> HuziResult<inkwell::values::BasicValueEnum<'ctx>> {
        match lit {
            Literal::Int(n) => Ok(self.context.i32_type().const_int(*n as u64, false).into()),
            Literal::Float(f) => Ok(self.context.f64_type().const_float(*f).into()),
            Literal::Bool(b) => {
                let val = if *b { 1i64 } else { 0i64 };
                Ok(self.context.i32_type().const_int(val as u64, false).into())
            }
            Literal::String(s) => {
                let ptr = unsafe { self.builder.build_global_string(s, "str").unwrap() };
                Ok(ptr.as_pointer_value().into())
            }
            Literal::Char(c) => Ok(self.context.i8_type().const_int(*c as u64, false).into()),
        }
    }

    fn compile_binary(
        &mut self,
        expr: &BinaryExpr,
    ) -> HuziResult<inkwell::values::BasicValueEnum<'ctx>> {
        let left = self.compile_expr(&expr.left)?;
        let right = self.compile_expr(&expr.right)?;

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
            BinOp::Mod => self
                .builder
                .build_int_signed_rem(left.into_int_value(), right.into_int_value(), "mod")
                .unwrap()
                .into(),
            BinOp::Eq => {
                if left.is_int_value() {
                    self.builder
                        .build_int_compare(
                            inkwell::IntPredicate::EQ,
                            left.into_int_value(),
                            right.into_int_value(),
                            "eq",
                        )
                        .unwrap()
                        .into()
                } else {
                    self.builder
                        .build_float_compare(
                            inkwell::FloatPredicate::OEQ,
                            left.into_float_value(),
                            right.into_float_value(),
                            "eq",
                        )
                        .unwrap()
                        .into()
                }
            }
            BinOp::Neq => {
                if left.is_int_value() {
                    self.builder
                        .build_int_compare(
                            inkwell::IntPredicate::NE,
                            left.into_int_value(),
                            right.into_int_value(),
                            "neq",
                        )
                        .unwrap()
                        .into()
                } else {
                    self.builder
                        .build_float_compare(
                            inkwell::FloatPredicate::ONE,
                            left.into_float_value(),
                            right.into_float_value(),
                            "neq",
                        )
                        .unwrap()
                        .into()
                }
            }
            BinOp::Lt => {
                if left.is_int_value() {
                    self.builder
                        .build_int_compare(
                            inkwell::IntPredicate::SLT,
                            left.into_int_value(),
                            right.into_int_value(),
                            "lt",
                        )
                        .unwrap()
                        .into()
                } else {
                    self.builder
                        .build_float_compare(
                            inkwell::FloatPredicate::OLT,
                            left.into_float_value(),
                            right.into_float_value(),
                            "lt",
                        )
                        .unwrap()
                        .into()
                }
            }
            BinOp::Le => {
                if left.is_int_value() {
                    self.builder
                        .build_int_compare(
                            inkwell::IntPredicate::SLE,
                            left.into_int_value(),
                            right.into_int_value(),
                            "le",
                        )
                        .unwrap()
                        .into()
                } else {
                    self.builder
                        .build_float_compare(
                            inkwell::FloatPredicate::OLE,
                            left.into_float_value(),
                            right.into_float_value(),
                            "le",
                        )
                        .unwrap()
                        .into()
                }
            }
            BinOp::Gt => {
                if left.is_int_value() {
                    self.builder
                        .build_int_compare(
                            inkwell::IntPredicate::SGT,
                            left.into_int_value(),
                            right.into_int_value(),
                            "gt",
                        )
                        .unwrap()
                        .into()
                } else {
                    self.builder
                        .build_float_compare(
                            inkwell::FloatPredicate::OGT,
                            left.into_float_value(),
                            right.into_float_value(),
                            "gt",
                        )
                        .unwrap()
                        .into()
                }
            }
            BinOp::Ge => {
                if left.is_int_value() {
                    self.builder
                        .build_int_compare(
                            inkwell::IntPredicate::SGE,
                            left.into_int_value(),
                            right.into_int_value(),
                            "ge",
                        )
                        .unwrap()
                        .into()
                } else {
                    self.builder
                        .build_float_compare(
                            inkwell::FloatPredicate::OGE,
                            left.into_float_value(),
                            right.into_float_value(),
                            "ge",
                        )
                        .unwrap()
                        .into()
                }
            }
            BinOp::And => self
                .builder
                .build_and(left.into_int_value(), right.into_int_value(), "and")
                .unwrap()
                .into(),
            BinOp::Or => self
                .builder
                .build_or(left.into_int_value(), right.into_int_value(), "or")
                .unwrap()
                .into(),
        };

        Ok(value)
    }

    fn compile_unary(
        &mut self,
        expr: &UnaryExpr,
    ) -> HuziResult<inkwell::values::BasicValueEnum<'ctx>> {
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
            UnOp::Not => self
                .builder
                .build_not(operand.into_int_value(), "not")
                .unwrap()
                .into(),
        };

        Ok(value)
    }

    fn compile_call(
        &mut self,
        expr: &CallExpr,
    ) -> HuziResult<inkwell::values::BasicValueEnum<'ctx>> {
        let callee_name = match &*expr.callee {
            Expr::Ident(name) => name.clone(),
            _ => return Err(HuziError::new_global("Expected function name")),
        };

        // Built-in functions
        if callee_name == "print" {
            return self.compile_print(&expr.arguments);
        }

        // Standard library functions
        if callee_name == "read_line" {
            return self.compile_read_line();
        }
        if callee_name == "read_int" {
            return self.compile_read_int();
        }
        if callee_name == "read_float" {
            return self.compile_read_float();
        }
        if callee_name == "len" {
            return self.compile_len(&expr.arguments);
        }
        if callee_name == "abs" {
            return self.compile_abs(&expr.arguments);
        }
        if callee_name == "sqrt" {
            return self.compile_sqrt(&expr.arguments);
        }
        if callee_name == "pow" {
            return self.compile_pow(&expr.arguments);
        }
        if callee_name == "sin" {
            return self.compile_sin(&expr.arguments);
        }
        if callee_name == "cos" {
            return self.compile_cos(&expr.arguments);
        }
        if callee_name == "concat" {
            return self.compile_concat(&expr.arguments);
        }
        if callee_name == "to_string" {
            return self.compile_to_string(&expr.arguments);
        }

        let function = if let Some(func) = self.functions.get(&callee_name) {
            *func
        } else {
            return Err(HuziError::new_global(format!(
                "Unknown function: {}",
                callee_name
            )));
        };

        let args: Vec<inkwell::values::BasicMetadataValueEnum> = expr
            .arguments
            .iter()
            .map(|a| self.compile_expr(a).map(|v| v.into()))
            .collect::<HuziResult<Vec<_>>>()?;

        let call = self.builder.build_call(function, &args, "call").unwrap();

        Ok(call.try_as_basic_value().unwrap_left().into())
    }

    fn compile_print(
        &mut self,
        arguments: &[Expr],
    ) -> HuziResult<inkwell::values::BasicValueEnum<'ctx>> {
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
            return Ok(call.try_as_basic_value().unwrap_left().into());
        }

        let mut format_string = String::new();
        let mut args: Vec<inkwell::values::BasicMetadataValueEnum> = Vec::new();

        for arg in arguments.iter() {
            let value = self.compile_expr(arg)?;

            match &arg {
                Expr::Literal(lit) => match lit {
                    Literal::String(s) => {
                        format_string.push_str(s);
                    }
                    Literal::Int(i) => {
                        // i64 使用 %ld, i32 使用 %d
                        if *i > i32::MAX as i64 || *i < i32::MIN as i64 {
                            format_string.push_str("%ld");
                        } else {
                            format_string.push_str("%d");
                        }
                        args.push(value.into());
                    }
                    Literal::Float(fl) => {
                        // f64 使用 %f, f32 使用 %g
                        let fl_val = *fl;
                        if fl_val as f32 as f64 == fl_val {
                            format_string.push_str("%g");
                        } else {
                            format_string.push_str("%f");
                        }
                        args.push(value.into());
                    }
                    Literal::Bool(b) => {
                        let bool_str = if *b { "true" } else { "false" };
                        let bool_ptr = unsafe {
                            self.builder
                                .build_global_string(bool_str, "bool_str")
                                .unwrap()
                        };
                        format_string.push_str("%s");
                        args.push(bool_ptr.as_pointer_value().into());
                    }
                    Literal::Char(c) => {
                        format_string.push(*c);
                    }
                },
                Expr::Ident(name) => {
                    if self.variables.get(name).is_some() {
                        // 根据 LLVM 类型确定格式说明符
                        if value.is_int_value() {
                            let int_val = value.into_int_value();
                            if int_val.get_type().get_bit_width() == 64 {
                                format_string.push_str("%ld");
                            } else {
                                format_string.push_str("%d");
                            }
                        } else if value.is_float_value() {
                            let float_val = value.into_float_value();
                            // f32 使用 %g, f64 使用 %f
                            if float_val.get_type() == self.context.f32_type() {
                                format_string.push_str("%g");
                            } else {
                                format_string.push_str("%f");
                            }
                        } else if value.is_pointer_value() {
                            format_string.push_str("%s");
                        }
                        args.push(value.into());
                    } else if self.functions.get(name).is_some() {
                        format_string.push_str("%p");
                        args.push(value.into());
                    }
                }
                _ => {
                    // 对于复杂表达式，根据值类型确定格式
                    if value.is_int_value() {
                        let int_val = value.into_int_value();
                        if int_val.get_type().get_bit_width() == 64 {
                            format_string.push_str("%ld");
                        } else {
                            format_string.push_str("%d");
                        }
                    } else if value.is_float_value() {
                        let float_val = value.into_float_value();
                        if float_val.get_type() == self.context.f32_type() {
                            format_string.push_str("%g");
                        } else {
                            format_string.push_str("%f");
                        }
                    } else if value.is_pointer_value() {
                        format_string.push_str("%s");
                    } else {
                        format_string.push_str("%d");
                    }
                    args.push(value.into());
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

        Ok(call.try_as_basic_value().unwrap_left().into())
    }

    fn compile_assign(
        &mut self,
        expr: &AssignExpr,
    ) -> HuziResult<inkwell::values::BasicValueEnum<'ctx>> {
        let target_name = match &*expr.target {
            Expr::Ident(name) => name.clone(),
            _ => return Err(HuziError::new_global("Invalid assignment target")),
        };

        let value = self.compile_expr(&expr.value)?;

        if let Some((ptr, _)) = self.variables.get(&target_name) {
            self.builder.build_store(*ptr, value).unwrap();
        } else {
            return Err(HuziError::new_global(format!(
                "Unknown variable: {}",
                target_name
            )));
        }

        Ok(value)
    }

    fn type_to_llvm(&self, ty: &Type) -> HuziResult<inkwell::types::BasicTypeEnum<'ctx>> {
        match ty {
            Type::I32 => Ok(self.context.i32_type().into()),
            Type::I64 => Ok(self.context.i64_type().into()),
            Type::U32 => Ok(self.context.i32_type().into()),
            Type::U64 => Ok(self.context.i64_type().into()),
            Type::F32 => Ok(self.context.f32_type().into()),
            Type::F64 => Ok(self.context.f64_type().into()),
            Type::Bool => Ok(self.context.i8_type().into()),
            Type::Char => Ok(self.context.i8_type().into()),
            Type::Str => Ok(self
                .context
                .ptr_type(AddressSpace::default())
                .into()),
            Type::Unit => Ok(self.context.i32_type().into()),
            Type::Named(name) if name == "i32" => Ok(self.context.i32_type().into()),
            Type::Named(name) if name == "i64" => Ok(self.context.i64_type().into()),
            Type::Named(name) if name == "f32" => Ok(self.context.f32_type().into()),
            Type::Named(name) if name == "f64" => Ok(self.context.f64_type().into()),
            Type::Named(name) if name == "bool" => Ok(self.context.i8_type().into()),
            Type::Named(name) if name == "str" => Ok(self
                .context
                .ptr_type(AddressSpace::default())
                .into()),
            _ => Err(HuziError::new_global("Unsupported type")),
        }
    }

    fn build_alloca(
        &self,
        ty: inkwell::types::BasicTypeEnum<'ctx>,
        name: &str,
    ) -> HuziResult<PointerValue<'ctx>> {
        let function = self
            .builder
            .get_insert_block()
            .unwrap()
            .get_parent()
            .unwrap();
        let entry = function.get_first_basic_block().unwrap();
        let builder = self.context.create_builder();
        builder.position_at_end(entry);
        Ok(builder.build_alloca(ty, name).unwrap())
    }

    // ==================== Standard Library Functions ====================

    fn compile_read_line(&mut self) -> HuziResult<inkwell::values::BasicValueEnum<'ctx>> {
        let malloc_fn = self.module.get_function("malloc").unwrap();

        // Allocate buffer (256 bytes)
        let buffer_size = self.context.i32_type().const_int(256, false);
        let buffer = self
            .builder
            .build_call(malloc_fn, &[buffer_size.into()], "buffer")
            .unwrap()
            .try_as_basic_value()
            .unwrap_left()
            .into_pointer_value();

        // Simple implementation: just return buffer pointer
        // A full implementation would need a loop to read characters

        Ok(buffer.into())
    }

    fn compile_read_int(&mut self) -> HuziResult<inkwell::values::BasicValueEnum<'ctx>> {
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

    fn compile_read_float(&mut self) -> HuziResult<inkwell::values::BasicValueEnum<'ctx>> {
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

    fn compile_len(&mut self, arguments: &[Expr]) -> HuziResult<inkwell::values::BasicValueEnum<'ctx>> {
        if arguments.is_empty() {
            return Err(HuziError::new_global("len() requires 1 argument"));
        }

        let strlen_fn = self.module.get_function("strlen").unwrap();
        let arg = self.compile_expr(&arguments[0])?;

        let len = self
            .builder
            .build_call(strlen_fn, &[arg.into()], "str_len")
            .unwrap()
            .try_as_basic_value()
            .unwrap_left();

        Ok(len)
    }

    fn compile_abs(&mut self, arguments: &[Expr]) -> HuziResult<inkwell::values::BasicValueEnum<'ctx>> {
        if arguments.is_empty() {
            return Err(HuziError::new_global("abs() requires 1 argument"));
        }

        let fabs_fn = self.module.get_function("fabs").unwrap();
        let arg = self.compile_expr(&arguments[0])?;

        // Convert to f64 if needed
        let arg_f64 = if arg.is_int_value() {
            // Integer to float: use sitofp
            let int_val = arg.into_int_value();
            self.builder
                .build_signed_int_to_float(int_val, self.context.f64_type(), "to_f64")
                .unwrap()
        } else if arg.is_float_value() {
            let float_val = arg.into_float_value();
            if float_val.get_type() == self.context.f32_type() {
                self.builder
                    .build_float_cast(float_val, self.context.f64_type(), "to_f64")
                    .unwrap()
            } else {
                float_val
            }
        } else {
            return Err(HuziError::new_global("abs() requires numeric argument"));
        };

        let result = self
            .builder
            .build_call(fabs_fn, &[arg_f64.into()], "abs_result")
            .unwrap()
            .try_as_basic_value()
            .unwrap_left();

        Ok(result)
    }

    fn compile_sqrt(&mut self, arguments: &[Expr]) -> HuziResult<inkwell::values::BasicValueEnum<'ctx>> {
        if arguments.is_empty() {
            return Err(HuziError::new_global("sqrt() requires 1 argument"));
        }

        let sqrt_fn = self.module.get_function("sqrt").unwrap();
        let arg = self.compile_expr(&arguments[0])?;

        // Convert to f64 if needed
        let arg_f64 = if arg.is_int_value() {
            let int_val = arg.into_int_value();
            self.builder
                .build_signed_int_to_float(int_val, self.context.f64_type(), "to_f64")
                .unwrap()
        } else if arg.is_float_value() {
            let float_val = arg.into_float_value();
            if float_val.get_type() == self.context.f32_type() {
                self.builder
                    .build_float_cast(float_val, self.context.f64_type(), "to_f64")
                    .unwrap()
            } else {
                float_val
            }
        } else {
            return Err(HuziError::new_global("sqrt() requires numeric argument"));
        };

        let result = self
            .builder
            .build_call(sqrt_fn, &[arg_f64.into()], "sqrt_result")
            .unwrap()
            .try_as_basic_value()
            .unwrap_left();

        Ok(result)
    }

    fn compile_pow(&mut self, arguments: &[Expr]) -> HuziResult<inkwell::values::BasicValueEnum<'ctx>> {
        if arguments.len() < 2 {
            return Err(HuziError::new_global("pow() requires 2 arguments"));
        }

        let pow_fn = self.module.get_function("pow").unwrap();
        let base = self.compile_expr(&arguments[0])?;
        let exp = self.compile_expr(&arguments[1])?;

        // Convert base to f64 if needed
        let base_f64 = if base.is_int_value() {
            let int_val = base.into_int_value();
            self.builder
                .build_signed_int_to_float(int_val, self.context.f64_type(), "base_f64")
                .unwrap()
        } else if base.is_float_value() {
            let float_val = base.into_float_value();
            if float_val.get_type() == self.context.f32_type() {
                self.builder
                    .build_float_cast(float_val, self.context.f64_type(), "base_f64")
                    .unwrap()
            } else {
                float_val
            }
        } else {
            return Err(HuziError::new_global("pow() requires numeric arguments"));
        };

        // Convert exp to f64 if needed
        let exp_f64 = if exp.is_int_value() {
            let int_val = exp.into_int_value();
            self.builder
                .build_signed_int_to_float(int_val, self.context.f64_type(), "exp_f64")
                .unwrap()
        } else if exp.is_float_value() {
            let float_val = exp.into_float_value();
            if float_val.get_type() == self.context.f32_type() {
                self.builder
                    .build_float_cast(float_val, self.context.f64_type(), "exp_f64")
                    .unwrap()
            } else {
                float_val
            }
        } else {
            return Err(HuziError::new_global("pow() requires numeric arguments"));
        };

        let result = self
            .builder
            .build_call(pow_fn, &[base_f64.into(), exp_f64.into()], "pow_result")
            .unwrap()
            .try_as_basic_value()
            .unwrap_left();

        Ok(result)
    }

    fn compile_sin(&mut self, arguments: &[Expr]) -> HuziResult<inkwell::values::BasicValueEnum<'ctx>> {
        if arguments.is_empty() {
            return Err(HuziError::new_global("sin() requires 1 argument"));
        }

        let sin_fn = self.module.get_function("sin").unwrap();
        let arg = self.compile_expr(&arguments[0])?;

        // Convert to f64 if needed
        let arg_f64 = if arg.is_int_value() {
            let int_val = arg.into_int_value();
            self.builder
                .build_signed_int_to_float(int_val, self.context.f64_type(), "to_f64")
                .unwrap()
        } else if arg.is_float_value() {
            let float_val = arg.into_float_value();
            if float_val.get_type() == self.context.f32_type() {
                self.builder
                    .build_float_cast(float_val, self.context.f64_type(), "to_f64")
                    .unwrap()
            } else {
                float_val
            }
        } else {
            return Err(HuziError::new_global("sin() requires numeric argument"));
        };

        let result = self
            .builder
            .build_call(sin_fn, &[arg_f64.into()], "sin_result")
            .unwrap()
            .try_as_basic_value()
            .unwrap_left();

        Ok(result)
    }

    fn compile_cos(&mut self, arguments: &[Expr]) -> HuziResult<inkwell::values::BasicValueEnum<'ctx>> {
        if arguments.is_empty() {
            return Err(HuziError::new_global("cos() requires 1 argument"));
        }

        let cos_fn = self.module.get_function("cos").unwrap();
        let arg = self.compile_expr(&arguments[0])?;

        // Convert to f64 if needed
        let arg_f64 = if arg.is_int_value() {
            let int_val = arg.into_int_value();
            self.builder
                .build_signed_int_to_float(int_val, self.context.f64_type(), "to_f64")
                .unwrap()
        } else if arg.is_float_value() {
            let float_val = arg.into_float_value();
            if float_val.get_type() == self.context.f32_type() {
                self.builder
                    .build_float_cast(float_val, self.context.f64_type(), "to_f64")
                    .unwrap()
            } else {
                float_val
            }
        } else {
            return Err(HuziError::new_global("cos() requires numeric argument"));
        };

        let result = self
            .builder
            .build_call(cos_fn, &[arg_f64.into()], "cos_result")
            .unwrap()
            .try_as_basic_value()
            .unwrap_left();

        Ok(result)
    }

    fn compile_concat(&mut self, arguments: &[Expr]) -> HuziResult<inkwell::values::BasicValueEnum<'ctx>> {
        if arguments.len() < 2 {
            return Err(HuziError::new_global("concat() requires 2 arguments"));
        }

        let malloc_fn = self.module.get_function("malloc").unwrap();
        let strcpy_fn = self.module.get_function("strcpy").unwrap();
        let strlen_fn = self.module.get_function("strlen").unwrap();

        let arg1 = self.compile_expr(&arguments[0])?;
        let arg2 = self.compile_expr(&arguments[1])?;

        // Calculate total length
        let len1 = self
            .builder
            .build_call(strlen_fn, &[arg1.into()], "len1")
            .unwrap()
            .try_as_basic_value()
            .unwrap_left()
            .into_int_value();

        let len2 = self
            .builder
            .build_call(strlen_fn, &[arg2.into()], "len2")
            .unwrap()
            .try_as_basic_value()
            .unwrap_left()
            .into_int_value();

        let total_len = self
            .builder
            .build_int_add(len1, len2, "total_len")
            .unwrap();

        // Allocate buffer (len1 + len2 + 1 for null terminator)
        let one = self.context.i32_type().const_int(1, false);
        let alloc_size = self.builder.build_int_add(total_len, one, "alloc_size").unwrap();
        let buffer = self
            .builder
            .build_call(malloc_fn, &[alloc_size.into()], "concat_buffer")
            .unwrap()
            .try_as_basic_value()
            .unwrap_left()
            .into_pointer_value();

        // Copy first string
        self.builder
            .build_call(strcpy_fn, &[buffer.into(), arg1.into()], "copy1")
            .unwrap();

        // Get pointer to end of first string
        let buffer_end = unsafe {
            self.builder
                .build_gep(
                    self.context.i8_type(),
                    buffer,
                    &[len1],
                    "buffer_end",
                )
                .unwrap()
        };

        // Copy second string
        self.builder
            .build_call(strcpy_fn, &[buffer_end.into(), arg2.into()], "copy2")
            .unwrap();

        Ok(buffer.into())
    }

    fn compile_to_string(&mut self, arguments: &[Expr]) -> HuziResult<inkwell::values::BasicValueEnum<'ctx>> {
        if arguments.is_empty() {
            return Err(HuziError::new_global("to_string() requires 1 argument"));
        }

        let malloc_fn = self.module.get_function("malloc").unwrap();
        let sprintf_fn_type = self.context.i32_type().fn_type(
            &[
                self.context.ptr_type(AddressSpace::default()).into(),
                self.context.ptr_type(AddressSpace::default()).into(),
            ],
            true,
        );
        
        // Get or create sprintf function
        let sprintf_fn = match self.module.get_function("sprintf") {
            Some(f) => f,
            None => self.module.add_function("sprintf", sprintf_fn_type, None),
        };

        let arg = self.compile_expr(&arguments[0])?;

        // Determine format string based on type
        let (format_str, arg_value) = if arg.is_int_value() {
            let int_val = arg.into_int_value();
            if int_val.get_type().get_bit_width() == 64 {
                (unsafe { self.builder.build_global_string("%ld", "fmt_i64").unwrap() }, arg)
            } else {
                (unsafe { self.builder.build_global_string("%d", "fmt_i32").unwrap() }, arg)
            }
        } else if arg.is_float_value() {
            let float_val = arg.into_float_value();
            if float_val.get_type() == self.context.f32_type() {
                (unsafe { self.builder.build_global_string("%g", "fmt_f32").unwrap() }, arg)
            } else {
                (unsafe { self.builder.build_global_string("%f", "fmt_f64").unwrap() }, arg)
            }
        } else {
            return Err(HuziError::new_global("to_string() unsupported type"));
        };

        // Allocate buffer (32 bytes should be enough for most numbers)
        let buffer_size = self.context.i32_type().const_int(32, false);
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
                &[buffer.into(), format_str.as_pointer_value().into(), arg_value.into()],
                "sprintf",
            )
            .unwrap();

        Ok(buffer.into())
    }

    // ==================== Array Functions ====================

    fn compile_array_index(
        &mut self,
        expr: &huzi_ast::ArrayIndexExpr,
    ) -> HuziResult<inkwell::values::BasicValueEnum<'ctx>> {
        let array_ptr = self.compile_expr(&expr.array)?;
        let index_val = self.compile_expr(&expr.index)?;

        // Get the pointer value
        let array_ptr_val = if array_ptr.is_pointer_value() {
            array_ptr.into_pointer_value()
        } else {
            return Err(HuziError::new_global("Expected array pointer"));
        };

        // Convert index to i32 if needed
        let index_i32 = if index_val.is_int_value() {
            let int_val = index_val.into_int_value();
            if int_val.get_type().get_bit_width() == 64 {
                self.builder
                    .build_int_truncate_or_bit_cast(
                        int_val,
                        self.context.i32_type(),
                        "index_i32",
                    )
                    .unwrap()
            } else {
                int_val
            }
        } else {
            return Err(HuziError::new_global("Array index must be integer"));
        };

        // Build GEP to get element pointer
        let elem_ptr = unsafe {
            self.builder
                .build_gep(
                    self.context.i32_type(), // element type (assuming i32 for now)
                    array_ptr_val,
                    &[index_i32],
                    "elem_ptr",
                )
                .unwrap()
        };

        // Load the element value
        let elem_type = self.context.i32_type(); // assuming i32 for now
        let loaded = self
            .builder
            .build_load(elem_type, elem_ptr, "load_elem")
            .unwrap();

        Ok(loaded.into())
    }

    fn compile_array_literal(
        &mut self,
        elements: &[Expr],
    ) -> HuziResult<inkwell::values::BasicValueEnum<'ctx>> {
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

        // Create array constant using const_named_struct or build_insert_value
        // For simplicity, we'll create the array in memory using alloca and stores
        
        // Allocate space for the array
        let array_ptr = self.build_alloca(array_type.into(), "array")?;

        // Store each element
        for (i, val) in elem_values.iter().enumerate() {
            let index = self.context.i32_type().const_int(i as u64, false);
            let elem_ptr = unsafe {
                self.builder
                    .build_gep(elem_type, array_ptr, &[index], "elem_ptr")
                    .unwrap()
            };
            self.builder.build_store(elem_ptr, *val).unwrap();
        }

        Ok(array_ptr.into())
    }

    pub fn print_llvm_ir(&self) -> String {
        self.module.print_to_string().to_string()
    }

    pub fn verify(&self) -> bool {
        self.module.verify().is_ok()
    }

    pub fn write_ir_to_file(&self, path: &str) -> Result<(), std::io::Error> {
        std::fs::write(path, self.module.print_to_string().to_string())
    }
}
