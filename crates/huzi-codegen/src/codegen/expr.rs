use super::{CodeGen, StructFieldInfo};
use inkwell::values::PointerValue;
use huzi_ast::*;
use huzi_error::{HuziError, Result};

impl<'ctx> CodeGen<'ctx> {
    pub(super) fn compile_expr(&mut self, expr: &Expr) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
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
            Expr::FieldAccess(fa) => self.compile_field_access(fa),
            Expr::StructLiteral(sl) => self.compile_struct_literal(sl),
            Expr::EnumConstruct(ec) => self.compile_enum_construct(ec),
            Expr::Match(m) => self.compile_match_expr(m),
        }
    }

    pub(super) fn compile_literal(&self, lit: &Literal) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
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

    pub(super) fn compile_binary(
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

    pub(super) fn build_int_or_float_compare(
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

    pub(super) fn compile_short_circuit(
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

    pub(super) fn compile_unary(
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

    pub(super) fn compile_call(
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
            "sqrt" => return self.compile_libm_unary("sqrt", &expr.arguments),
            "pow" => return self.compile_pow(&expr.arguments),
            "sin" => return self.compile_libm_unary("sin", &expr.arguments),
            "cos" => return self.compile_libm_unary("cos", &expr.arguments),
            "tan" => return self.compile_libm_unary("tan", &expr.arguments),
            "floor" => return self.compile_libm_unary("floor", &expr.arguments),
            "ceil" => return self.compile_libm_unary("ceil", &expr.arguments),
            "round" => return self.compile_libm_unary("round", &expr.arguments),
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

    pub(super) fn compile_assign(
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
                self.ensure_mutable(&expr.target)?;
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
            Expr::FieldAccess(_) => {
                self.ensure_mutable(&expr.target)?;
                let (field_ptr, field_ty) = self.compile_addr(&expr.target)?;
                let value = self.coerce_value(field_ty, value)?;
                self.builder.build_store(field_ptr, value).unwrap();
                Ok(value)
            }
            _ => Err(HuziError::new_global("Invalid assignment target")),
        }
    }


    pub(super) fn compile_addr(
        &mut self,
        expr: &Expr,
    ) -> Result<(PointerValue<'ctx>, inkwell::types::BasicTypeEnum<'ctx>)> {
        match expr {
            Expr::Ident(name) => {
                let slot = self
                    .scope_lookup(name)
                    .ok_or_else(|| HuziError::new_global(format!("Unknown variable: {}", name)))?;
                Ok((slot.ptr, slot.ty))
            }
            Expr::FieldAccess(fa) => {
                let (base_ptr, base_ty) = self.compile_addr(&fa.base)?;
                self.gep_field(base_ptr, base_ty, &fa.field)
            }
            Expr::ArrayIndex(idx_expr) => {
                let array_ptr = self.compile_expr(&idx_expr.array)?;
                let array_ptr = if array_ptr.is_pointer_value() {
                    array_ptr.into_pointer_value()
                } else {
                    return Err(HuziError::new_global("Indexed value is not an array"));
                };

                let elem_type = self.resolve_elem_type(&idx_expr.array, None)?;
                let index_val = self.compile_expr(&idx_expr.index)?;
                let index_i32 = self.coerce_index(index_val)?;

                let elem_ptr = unsafe {
                    self.builder
                        .build_gep(elem_type, array_ptr, &[index_i32], "elem_ptr")
                        .unwrap()
                };
                Ok((elem_ptr, elem_type))
            }
            _ => {
                // Rvalue base (e.g. a function call or enum constructor):
                // spill it to a temporary so it has an address.
                let value = self.compile_expr(expr)?;
                let ty = value.get_type();
                let tmp = self.build_alloca(ty, "rvalue_tmp")?;
                self.builder.build_store(tmp, value).unwrap();
                Ok((tmp, ty))
            }
        }
    }

    /// GEP to a named field of the struct value stored at `base_ptr`.
    pub(super) fn gep_field(
        &self,
        base_ptr: PointerValue<'ctx>,
        base_ty: inkwell::types::BasicTypeEnum<'ctx>,
        field: &str,
    ) -> Result<(PointerValue<'ctx>, inkwell::types::BasicTypeEnum<'ctx>)> {
        let (_, fields) = self
            .struct_def_by_type(base_ty)
            .ok_or_else(|| HuziError::new_global("Value has no fields (not a struct)"))?;

        let (index, info) = fields
            .iter()
            .enumerate()
            .find(|(_, info)| info.name == field)
            .ok_or_else(|| HuziError::new_global(format!("Struct has no field '{}'", field)))?;

        let field_ptr = self
            .builder
            .build_struct_gep(base_ty.into_struct_type(), base_ptr, index as u32, "field_ptr")
            .unwrap();
        Ok((field_ptr, info.ty))
    }

    /// Find a registered struct definition by its LLVM type.
    pub(super) fn struct_def_by_type(
        &self,
        ty: inkwell::types::BasicTypeEnum<'ctx>,
    ) -> Option<&(inkwell::types::StructType<'ctx>, Vec<StructFieldInfo<'ctx>>)> {
        let st = match ty {
            inkwell::types::BasicTypeEnum::StructType(st) => st,
            _ => return None,
        };
        self.structs.values().find(|(def_st, _)| *def_st == st)
    }

    /// Best-effort struct definition lookup for an expression, following
    /// variables and field chains.
    pub(super) fn struct_def_of_expr(
        &self,
        expr: &Expr,
    ) -> Option<&(inkwell::types::StructType<'ctx>, Vec<StructFieldInfo<'ctx>>)> {
        match expr {
            Expr::Ident(name) => {
                let slot = self.scope_lookup(name)?;
                self.struct_def_by_type(slot.ty)
            }
            Expr::FieldAccess(fa) => {
                let (_, fields) = self.struct_def_of_expr(&fa.base)?;
                let info = fields.iter().find(|info| info.name == fa.field)?;
                self.struct_def_by_type(info.ty)
            }
            _ => None,
        }
    }

    /// The root of an lvalue chain must be a mutable variable.
    pub(super) fn ensure_mutable(&self, expr: &Expr) -> Result<()> {
        match expr {
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
                Ok(())
            }
            Expr::FieldAccess(fa) => self.ensure_mutable(&fa.base),
            Expr::ArrayIndex(idx) => self.ensure_mutable(&idx.array),
            _ => Ok(()),
        }
    }

    pub(super) fn compile_field_access(
        &mut self,
        expr: &FieldAccessExpr,
    ) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        let (base_ptr, base_ty) = self.compile_addr(&expr.base)?;
        let (field_ptr, field_ty) = self.gep_field(base_ptr, base_ty, &expr.field)?;
        let loaded = self
            .builder
            .build_load(field_ty, field_ptr, "field")
            .unwrap();
        Ok(loaded)
    }

}
