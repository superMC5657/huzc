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
    variables: HashMap<String, PointerValue<'ctx>>,
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

        for stmt in &program.statements {
            self.compile_stmt(stmt)?;
        }

        Ok(())
    }

    fn prelude(&mut self) -> HuziResult<()> {
        let print_fn = self.context.i32_type().fn_type(
            &[self
                .context
                .i8_type()
                .ptr_type(AddressSpace::default())
                .into()],
            false,
        );
        self.module.add_function("printf", print_fn, None);

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
            let alloca = self.build_alloca(value.get_type(), &stmt.name).unwrap();
            self.builder.build_store(alloca, value).unwrap();
            self.variables.insert(stmt.name.clone(), alloca);
        }
        Ok(())
    }

    fn compile_fn(&mut self, stmt: &FnStmt) -> HuziResult<()> {
        let fn_type = if let Some(ret_type) = &stmt.return_type {
            self.type_to_llvm(ret_type)?.fn_type(&[], false)
        } else {
            self.context.i32_type().fn_type(&[], false)
        };

        let function = self.module.add_function(&stmt.name, fn_type, None);
        let entry = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry);

        self.variables.clear();

        for (i, param) in stmt.params.iter().enumerate() {
            let arg = function.get_nth_param(i as u32).unwrap();
            let alloca = self.build_alloca(arg.get_type(), &param.name).unwrap();
            self.builder.build_store(alloca, arg).unwrap();
            self.variables.insert(param.name.clone(), alloca);
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
        let condition = self
            .builder
            .build_int_compare(
                inkwell::IntPredicate::NE,
                condition.into_int_value(),
                self.context.i32_type().const_int(0, false),
                "if_cond",
            )
            .unwrap();

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
        let i_alloca = self
            .build_alloca(self.context.i32_type().into(), &stmt.var_name)
            .unwrap();
        self.builder.build_store(i_alloca, start).unwrap();
        self.variables.insert(stmt.var_name.clone(), i_alloca);

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
                if let Some(ptr) = self.variables.get(name) {
                    let loaded = self
                        .builder
                        .build_load(self.context.i32_type(), *ptr, "load")
                        .unwrap();
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

        let function = if let Some(func) = self.functions.get(&callee_name) {
            *func
        } else if callee_name == "print" {
            self.module.get_function("printf").unwrap()
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

    fn compile_assign(
        &mut self,
        expr: &AssignExpr,
    ) -> HuziResult<inkwell::values::BasicValueEnum<'ctx>> {
        let target_name = match &*expr.target {
            Expr::Ident(name) => name.clone(),
            _ => return Err(HuziError::new_global("Invalid assignment target")),
        };

        let value = self.compile_expr(&expr.value)?;

        if let Some(ptr) = self.variables.get(&target_name) {
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
                .i8_type()
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
                .i8_type()
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
