use super::{CodeGen, EnumInfo, EnumVariantInfo, VarSlot};
use inkwell::types::BasicType;
use inkwell::values::PointerValue;
use huzi_ast::*;
use huzi_error::{HuziError, Result};

impl<'ctx> CodeGen<'ctx> {
    pub(super) fn compile_struct_literal(
        &mut self,
        expr: &StructLiteralExpr,
    ) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        let (struct_ty, fields) = self
            .structs
            .get(&expr.name)
            .cloned()
            .ok_or_else(|| HuziError::new_global(format!("Unknown struct: {}", expr.name)))?;

        for (i, (name, _)) in expr.fields.iter().enumerate() {
            if expr.fields[..i].iter().any(|(n, _)| n == name) {
                return Err(HuziError::new_global(format!(
                    "Duplicate field '{}' in struct literal",
                    name
                )));
            }
        }
        for (name, _) in &expr.fields {
            if !fields.iter().any(|info| info.name == *name) {
                return Err(HuziError::new_global(format!(
                    "Struct '{}' has no field '{}'",
                    expr.name, name
                )));
            }
        }
        for info in &fields {
            if !expr.fields.iter().any(|(n, _)| n == &info.name) {
                return Err(HuziError::new_global(format!(
                    "Missing field '{}' in struct literal for '{}'",
                    info.name, expr.name
                )));
            }
        }

        let tmp = self.build_alloca(struct_ty.into(), "struct_val")?;
        for (field_name, field_expr) in &expr.fields {
            let (index, info) = fields
                .iter()
                .enumerate()
                .find(|(_, info)| info.name == *field_name)
                .unwrap();
            let value = self.compile_expr(field_expr)?;
            let value = self.coerce_value(info.ty, value)?;
            let field_ptr = self
                .builder
                .build_struct_gep(struct_ty, tmp, index as u32, "field_ptr")
                .unwrap();
            self.builder.build_store(field_ptr, value).unwrap();
        }

        let loaded = self
            .builder
            .build_load(struct_ty, tmp, "struct_load")
            .unwrap();
        Ok(loaded)
    }

    // ==================== Enum Functions ====================

    pub(super) fn compile_enum_construct(
        &mut self,
        expr: &EnumConstructExpr,
    ) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        let (info, vinfo) = self.resolve_enum_variant(expr)?;

        let enum_st = match info.llvm {
            None => {
                // Simple enum: the value is the tag itself.
                if !expr.args.is_empty() {
                    return Err(HuziError::new_global(format!(
                        "Unit variant '{}::{}' takes no arguments",
                        expr.enum_name, expr.variant
                    )));
                }
                return Ok(self
                    .context
                    .i32_type()
                    .const_int(vinfo.tag as u64, false)
                    .into());
            }
            Some(st) => st,
        };

        self.build_data_enum_value(expr, &info, &vinfo, enum_st)
    }

    /// Look up the enum and the variant named by an `Enum::Variant` expr.
    fn resolve_enum_variant(
        &self,
        expr: &EnumConstructExpr,
    ) -> Result<(EnumInfo<'ctx>, EnumVariantInfo<'ctx>)> {
        let info = self
            .enums
            .get(&expr.enum_name)
            .cloned()
            .ok_or_else(|| HuziError::new_global(format!("Unknown enum: {}", expr.enum_name)))?;
        let vinfo = info
            .variants
            .iter()
            .find(|v| v.name == expr.variant)
            .cloned()
            .ok_or_else(|| {
                HuziError::new_global(format!(
                    "Enum '{}' has no variant '{}'",
                    expr.enum_name, expr.variant
                ))
            })?;
        Ok((info, vinfo))
    }

    /// Build `{ i32 tag, payload union }` for a data-carrying enum variant:
    /// check arity, store the discriminant, store the payload, and load the
    /// finished value.
    fn build_data_enum_value(
        &mut self,
        expr: &EnumConstructExpr,
        info: &EnumInfo<'ctx>,
        vinfo: &EnumVariantInfo<'ctx>,
        enum_st: inkwell::types::StructType<'ctx>,
    ) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        match (&vinfo.payload, expr.args.len()) {
            (None, 0) => {}
            (Some(_), 1) => {}
            (None, _) => {
                return Err(HuziError::new_global(format!(
                    "Unit variant '{}::{}' takes no arguments",
                    expr.enum_name, expr.variant
                )))
            }
            (Some(_), _) => {
                return Err(HuziError::new_global(format!(
                    "Variant '{}::{}' expects exactly 1 argument",
                    expr.enum_name, expr.variant
                )))
            }
        }

        let payload_union = info.payload_union.unwrap();
        let tmp = self.build_alloca(enum_st.into(), "enum_val")?;

        // Store the discriminant in field 0.
        let tag_ptr = self
            .builder
            .build_struct_gep(enum_st, tmp, 0, "enum_tag_ptr")
            .unwrap();
        self.builder
            .build_store(
                tag_ptr,
                self.context.i32_type().const_int(vinfo.tag as u64, false),
            )
            .unwrap();

        // Store the payload into the variant's slot of the union in field 1.
        if let Some(payload_ty) = vinfo.payload {
            let arg = self.compile_expr(&expr.args[0])?;
            let arg = self.coerce_value(payload_ty, arg)?;
            let union_ptr = self
                .builder
                .build_struct_gep(enum_st, tmp, 1, "enum_payload_ptr")
                .unwrap();
            let slot_ptr = self
                .builder
                .build_struct_gep(
                    payload_union,
                    union_ptr,
                    vinfo.payload_slot.unwrap(),
                    "enum_slot_ptr",
                )
                .unwrap();
            self.builder.build_store(slot_ptr, arg).unwrap();
        }

        let loaded = self
            .builder
            .build_load(enum_st, tmp, "enum_load")
            .unwrap();
        Ok(loaded)
    }

    pub(super) fn compile_match_expr(
        &mut self,
        expr: &MatchExpr,
    ) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        if expr.arms.is_empty() {
            return Err(HuziError::new_global("match must have at least one arm"));
        }

        // Address of the scrutinee (rvalues are spilled to a temporary).
        let (scrut_addr, scrut_ty) = self.compile_addr(&expr.scrutinee)?;

        // The enum being matched, named by the first variant pattern.
        let pat_enum_name = expr.arms.iter().find_map(|arm| match &arm.pattern {
            Pattern::Variant { enum_name, .. } => Some(enum_name.as_str()),
            Pattern::Wildcard => None,
        });

        // Data-carrying enums keep their tag in field 0 of the struct; simple
        // enums ARE the i32 tag, so the scrutinee value is the tag itself.
        if let Some(info) = self.enum_data_by_type(scrut_ty) {
            if let Some(pat) = pat_enum_name {
                if pat != info.name {
                    return Err(HuziError::new_global(format!(
                        "Match arms use '{}' but the scrutinee is '{}'",
                        pat, info.name
                    )));
                }
            }
            let st = info.llvm.unwrap();
            let info = info.clone();
            let tag_ptr = self
                .builder
                .build_struct_gep(st, scrut_addr, 0, "match_tag_ptr")
                .unwrap();
            let tag = self
                .builder
                .build_load(self.context.i32_type(), tag_ptr, "match_tag")
                .unwrap()
                .into_int_value();
            return self.compile_match_arms(tag, Some((st, scrut_addr)), Some(&info), &expr.arms);
        }

        if scrut_ty == self.context.i32_type().into() {
            let info = match pat_enum_name {
                Some(name) => {
                    let info = self.enums.get(name).cloned().ok_or_else(|| {
                        HuziError::new_global(format!("Unknown enum: {}", name))
                    })?;
                    if info.llvm.is_some() {
                        return Err(HuziError::new_global(format!(
                            "Cannot match '{}' against a plain i32 scrutinee; it carries data",
                            name
                        )));
                    }
                    Some(info)
                }
                None => None,
            };
            let tag = self
                .builder
                .build_load(self.context.i32_type(), scrut_addr, "match_tag")
                .unwrap()
                .into_int_value();
            return self.compile_match_arms(tag, None, info.as_ref(), &expr.arms);
        }

        Err(HuziError::new_global(
            "match scrutinee must be an enum value",
        ))
    }

    /// Compile the arm chain recursively: each variant arm branches on the tag
    /// and falls through to the remaining arms on mismatch.
    pub(super) fn compile_match_arms(
        &mut self,
        tag: inkwell::values::IntValue<'ctx>,
        data: Option<(
            inkwell::types::StructType<'ctx>,
            PointerValue<'ctx>,
        )>,
        info: Option<&EnumInfo<'ctx>>,
        arms: &[MatchArm],
    ) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        let (arm, rest) = arms.split_first().ok_or_else(|| {
            HuziError::new_global("match must have a wildcard arm `_`")
        })?;

        match &arm.pattern {
            Pattern::Wildcard => self.compile_block_value(&arm.body),
            Pattern::Variant {
                variant, binding, ..
            } => {
                let info = info.ok_or_else(|| {
                    HuziError::new_global("Cannot match variants without a known enum type")
                })?;
                let vinfo = find_variant(info, variant)?;

                let expected = self
                    .context
                    .i32_type()
                    .const_int(vinfo.tag as u64, false);
                let cond = self
                    .builder
                    .build_int_compare(inkwell::IntPredicate::EQ, tag, expected, "match_cond")
                    .unwrap();

                let function = self.current_function()?;
                let then_bb = self.context.append_basic_block(function, "match_arm");
                let else_bb = self.context.append_basic_block(function, "match_next");
                let merge_bb = self.context.append_basic_block(function, "match_merge");
                self.builder
                    .build_conditional_branch(cond, then_bb, else_bb)
                    .unwrap();

                // Matching arm: optionally bind the payload, evaluate the body.
                self.builder.position_at_end(then_bb);
                let then_val =
                    self.compile_match_arm_body(data, info, vinfo, binding, &arm.body)?;

                let result_ty = then_val.get_type();
                let result_ptr = self.build_alloca(result_ty, "match_val")?;
                self.builder.build_store(result_ptr, then_val).unwrap();
                self.builder
                    .build_unconditional_branch(merge_bb)
                    .unwrap();

                // Remaining arms run when the tag does not match.
                self.builder.position_at_end(else_bb);
                let else_val = self.compile_match_arms(tag, data, Some(info), rest)?;
                let else_val = self.coerce_value(result_ty, else_val)?;
                self.builder.build_store(result_ptr, else_val).unwrap();
                self.builder
                    .build_unconditional_branch(merge_bb)
                    .unwrap();

                self.builder.position_at_end(merge_bb);
                let result = self
                    .builder
                    .build_load(result_ty, result_ptr, "match_load")
                    .unwrap();
                Ok(result)
            }
        }
    }

    /// Bind the payload (if the pattern has a binding) and evaluate the arm
    /// body on the matching branch.
    fn compile_match_arm_body(
        &mut self,
        data: Option<(
            inkwell::types::StructType<'ctx>,
            PointerValue<'ctx>,
        )>,
        info: &EnumInfo<'ctx>,
        vinfo: &EnumVariantInfo<'ctx>,
        binding: &Option<String>,
        body: &Block,
    ) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        if let Some(bname) = binding {
            self.bind_match_payload(data, info, vinfo, bname)?;
        }
        let value = self.compile_block_value(body)?;
        if binding.is_some() {
            self.pop_scope();
        }
        Ok(value)
    }

    /// Enter a scope with the pattern binding bound to the variant's payload.
    pub(super) fn bind_match_payload(
        &mut self,
        data: Option<(inkwell::types::StructType<'ctx>, PointerValue<'ctx>)>,
        info: &EnumInfo<'ctx>,
        vinfo: &EnumVariantInfo<'ctx>,
        binding: &str,
    ) -> Result<()> {
        let (enum_st, scrut_addr) = data.ok_or_else(|| {
            HuziError::new_global(format!(
                "Variant '{}::{}' has no payload to bind",
                info.name, vinfo.name
            ))
        })?;
        let payload_ty = vinfo.payload.ok_or_else(|| {
            HuziError::new_global(format!(
                "Variant '{}::{}' has no payload to bind",
                info.name, vinfo.name
            ))
        })?;

        let union_st = info.payload_union.unwrap();
        let union_ptr = self
            .builder
            .build_struct_gep(enum_st, scrut_addr, 1, "bind_payload_ptr")
            .unwrap();
        let slot_ptr = self
            .builder
            .build_struct_gep(
                union_st,
                union_ptr,
                vinfo.payload_slot.unwrap(),
                "bind_slot_ptr",
            )
            .unwrap();

        // Arrays decay to pointers; keep the element type for indexing.
        let elem = match &vinfo.ast_payload {
            Some(Type::Array(elem_ty, _)) => Some(self.type_to_llvm(elem_ty)?),
            Some(Type::Str) => Some(self.context.i8_type().into()),
            _ => None,
        };
        let array_len = match &vinfo.ast_payload {
            Some(Type::Array(_, size)) => Some(*size as u32),
            _ => None,
        };

        self.push_scope();
        self.scope_insert(
            binding.to_string(),
            VarSlot {
                ptr: slot_ptr,
                ty: payload_ty,
                elem,
                array_len,
                mutable: false,
            },
        );
        Ok(())
    }

    // ==================== Array Functions ====================

    pub(super) fn compile_array_index(
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

    pub(super) fn compile_array_literal(
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

    pub(super) fn compile_if_expr(
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

}

/// Find the variant info for `variant` inside `info`.
fn find_variant<'ctx, 'a>(
    info: &'a EnumInfo<'ctx>,
    variant: &str,
) -> Result<&'a EnumVariantInfo<'ctx>> {
    info.variants.iter().find(|v| v.name == variant).ok_or_else(|| {
        HuziError::new_global(format!("Enum '{}' has no variant '{}'", info.name, variant))
    })
}
