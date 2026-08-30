use super::CodeGen;
use inkwell::AddressSpace;
use inkwell::values::PointerValue;
use huzi_ast::*;
use huzi_error::{HuziError, Result};

impl<'ctx> CodeGen<'ctx> {
    pub(super) fn prelude(&mut self) -> Result<()> {
        self.declare_libc_functions();
        self.declare_libm_functions();
        self.declare_arg_support();
        Ok(())
    }

    /// Declare the C runtime functions used by builtins (link to libc).
    fn declare_libc_functions(&mut self) {
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

        // strlen for string length
        let strlen_fn = self.context.i32_type().fn_type(
            &[self
                .context
                .ptr_type(AddressSpace::default())
                .into()],
            false,
        );
        self.module.add_function("strlen", strlen_fn, None);

        // strcmp for string comparison (==/!=/< etc. on str operands)
        let strcmp_fn = self.context.i32_type().fn_type(
            &[
                self.context.ptr_type(AddressSpace::default()).into(),
                self.context.ptr_type(AddressSpace::default()).into(),
            ],
            false,
        );
        self.module.add_function("strcmp", strcmp_fn, None);

        // exit for runtime error aborts (division by zero, out-of-bounds, ...)
        let exit_fn = self.context.void_type().fn_type(
            &[self.context.i32_type().into()],
            false,
        );
        self.module.add_function("exit", exit_fn, None);

        // strcpy for string copy
        let strcpy_fn = self.context.i32_type().fn_type(
            &[
                self.context.ptr_type(AddressSpace::default()).into(),
                self.context.ptr_type(AddressSpace::default()).into(),
            ],
            false,
        );
        self.module.add_function("strcpy", strcpy_fn, None);

        // SetConsoleOutputCP (kernel32) for UTF-8 console output on Windows
        if cfg!(windows) {
            let set_cp_fn =
                self.context.i32_type().fn_type(&[self.context.i32_type().into()], false);
            self.module.add_function("SetConsoleOutputCP", set_cp_fn, None);
        }
    }

    /// Declare the math functions (link to libm).
    fn declare_libm_functions(&mut self) {
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

        for name in ["tan", "floor", "ceil", "round"] {
            let f = self.context.f64_type().fn_type(&[self.context.f64_type().into()], false);
            self.module.add_function(name, f, None);
        }
    }

    // ==================== Scope helpers ====================

    /// Switch the Windows console to the UTF-8 code page (65001) so `printf`
    /// shows Chinese and other non-ASCII text correctly. String literals are
    /// stored as UTF-8 bytes; without this the console decodes them with its
    /// default code page (e.g. GBK) and prints mojibake. No-op elsewhere.
    pub(super) fn emit_console_utf8_setup(&mut self) {
        if !cfg!(windows) {
            return;
        }
        let set_cp_fn = self.module.get_function("SetConsoleOutputCP").unwrap();
        let utf8_cp = self.context.i32_type().const_int(65001, false);
        self.builder
            .build_call(set_cp_fn, &[utf8_cp.into()], "console_utf8")
            .unwrap();
    }

    pub(super) fn compile_print(
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
            self.format_print_value(value, &mut format_string, &mut args)?;
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

    /// Append one printed value to the printf format string and argument
    /// list, promoting/narrowing according to C varargs conventions.
    pub(super) fn format_print_value(
        &mut self,
        value: inkwell::values::BasicValueEnum<'ctx>,
        format_string: &mut String,
        args: &mut Vec<inkwell::values::BasicMetadataValueEnum<'ctx>>,
    ) -> Result<()> {
        // Tuples print as `(v1, v2, ...)` with each field formatted.
        if let inkwell::values::BasicValueEnum::StructValue(sv) = value {
            if self.is_tuple_type(sv.get_type()) {
                return self.format_tuple_value(sv.into(), format_string, args);
            }
        }

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

        Ok(())
    }

    /// Build a global "true"/"false" string selected by the given i1 condition.
    pub(super) fn build_bool_str(
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

    pub(super) fn compile_read_line(&mut self) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        let getchar_fn = self.module.get_function("getchar").unwrap();

        // Allocate buffer (256 bytes)
        let buffer = self.alloc_str_buffer(256)?;

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

        // Read one char per iteration until '''PLACEHOLDER''', EOF, or buffer full.
        self.builder.position_at_end(loop_block);
        let c = self
            .builder
            .build_call(getchar_fn, &[], "ch")
            .unwrap()
            .try_as_basic_value()
            .unwrap_left()
            .into_int_value();

        // Record EOF for is_eof(): getchar returns -1 at end of input.
        let eof_hit = self
            .builder
            .build_int_compare(
                inkwell::IntPredicate::EQ,
                c,
                i32_type.const_int(-1i64 as u64, true),
                "eof_hit",
            )
            .unwrap();
        self.mark_eof_flag(eof_hit);

        let idx = self
            .builder
            .build_load(i32_type, idx_ptr, "idx")
            .unwrap()
            .into_int_value();

        let cont = self.read_line_continue(c, idx, i32_type)?;

        self.builder
            .build_conditional_branch(cont, store_block, done_block)
            .unwrap();

        self.read_line_store(buffer, idx_ptr, idx, c, i32_type, store_block, loop_block)?;

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

    /// Whether the read loop should keep going: space left in the buffer,
    /// current char is not a newline, and not EOF.
    fn read_line_continue(
        &mut self,
        c: inkwell::values::IntValue<'ctx>,
        idx: inkwell::values::IntValue<'ctx>,
        i32_type: inkwell::types::IntType<'ctx>,
    ) -> Result<inkwell::values::IntValue<'ctx>> {
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
        Ok(cont)
    }

    /// Emit the store block: truncate the char to i8, write it at the current
    /// index, bump the index, and jump back to the loop header.
    fn read_line_store(
        &mut self,
        buffer: PointerValue<'ctx>,
        idx_ptr: PointerValue<'ctx>,
        idx: inkwell::values::IntValue<'ctx>,
        c: inkwell::values::IntValue<'ctx>,
        i32_type: inkwell::types::IntType<'ctx>,
        store_block: inkwell::basic_block::BasicBlock<'ctx>,
        loop_block: inkwell::basic_block::BasicBlock<'ctx>,
    ) -> Result<()> {
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
        Ok(())
    }

    pub(super) fn compile_read_int(&mut self) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        let scanf_fn = self.module.get_function("scanf").unwrap();

        // Format string for %d
        let format_str = unsafe {
            self.builder
                .build_global_string("%d", "scanf_format_int")
                .unwrap()
        };

        // Allocate space for int
        let int_ptr = self.build_alloca(self.context.i32_type().into(), "int_input")?;

        let scanf_ret = self
            .builder
            .build_call(
                scanf_fn,
                &[
                    format_str.as_pointer_value().into(),
                    int_ptr.into(),
                ],
                "scanf_int",
            )
            .unwrap()
            .try_as_basic_value()
            .unwrap_left()
            .into_int_value();

        // Record EOF for is_eof(): scanf returns -1 when input ends.
        let eof_hit = self
            .builder
            .build_int_compare(
                inkwell::IntPredicate::EQ,
                scanf_ret,
                self.context.i32_type().const_int(-1i64 as u64, true),
                "eof_hit",
            )
            .unwrap();
        self.mark_eof_flag(eof_hit);

        let value = self
            .builder
            .build_load(self.context.i32_type(), int_ptr, "int_value")
            .unwrap();

        Ok(value)
    }

    pub(super) fn compile_read_float(&mut self) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        let scanf_fn = self.module.get_function("scanf").unwrap();

        // Format string for %lf
        let format_str = unsafe {
            self.builder
                .build_global_string("%lf", "scanf_format_float")
                .unwrap()
        };

        // Allocate space for double
        let float_ptr = self.build_alloca(self.context.f64_type().into(), "float_input")?;

        let scanf_ret = self
            .builder
            .build_call(
                scanf_fn,
                &[
                    format_str.as_pointer_value().into(),
                    float_ptr.into(),
                ],
                "scanf_float",
            )
            .unwrap()
            .try_as_basic_value()
            .unwrap_left()
            .into_int_value();

        // Record EOF for is_eof(): scanf returns -1 when input ends.
        let eof_hit = self
            .builder
            .build_int_compare(
                inkwell::IntPredicate::EQ,
                scanf_ret,
                self.context.i32_type().const_int(-1i64 as u64, true),
                "eof_hit",
            )
            .unwrap();
        self.mark_eof_flag(eof_hit);

        let value = self
            .builder
            .build_load(self.context.f64_type(), float_ptr, "float_value")
            .unwrap();

        Ok(value)
    }

    pub(super) fn compile_len(&mut self, arguments: &[Expr]) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
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

        // len(s.arr) on a struct array field uses the declared array size.
        if let Expr::FieldAccess(fa) = &arguments[0] {
            if let Some((_, fields)) = self.struct_def_of_expr(&fa.base) {
                if let Some(info) = fields.iter().find(|info| info.name == fa.field) {
                    if let Type::Array(_, size) = &info.ast_ty {
                        return Ok(self.context.i32_type().const_int(*size as u64, false).into());
                    }
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

    pub(super) fn compile_abs(&mut self, arguments: &[Expr]) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
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

    /// Single-argument libm wrappers (sqrt/sin/cos/tan/floor/ceil/round):
    /// coerce the argument to f64, call the C function, return f64.
    pub(super) fn compile_libm_unary(
        &mut self,
        fn_name: &str,
        arguments: &[Expr],
    ) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        if arguments.len() != 1 {
            return Err(HuziError::new_global(format!(
                "{}() requires exactly 1 argument",
                fn_name
            )));
        }

        let f = self
            .module
            .get_function(fn_name)
            .ok_or_else(|| self.unknown_function_error(fn_name))?;
        let arg = self.compile_expr(&arguments[0])?;
        let arg_f64 = self.to_f64(arg, &format!("{}()", fn_name))?;

        let result = self
            .builder
            .build_call(f, &[arg_f64.into()], "libm_result")
            .unwrap()
            .try_as_basic_value()
            .unwrap_left();

        Ok(result)
    }

    pub(super) fn compile_pow(&mut self, arguments: &[Expr]) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
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

    /// Convert any numeric value to f64 for math builtins.
    pub(super) fn to_f64(
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

    pub(super) fn compile_concat(&mut self, arguments: &[Expr]) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        if arguments.len() < 2 {
            return Err(HuziError::new_global("concat() requires at least 2 arguments"));
        }

        let malloc_fn = self.module.get_function("malloc").unwrap();
        let strcpy_fn = self.module.get_function("strcpy").unwrap();

        let (arg_ptrs, arg_lens) = self.concat_string_args(arguments)?;

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

        self.concat_copy_into(buffer, strcpy_fn, &arg_ptrs, &arg_lens)?;

        Ok(buffer.into())
    }

    /// Evaluate the arguments, which must all be strings; return their
    /// pointers and strlen lengths.
    fn concat_string_args(
        &mut self,
        arguments: &[Expr],
    ) -> Result<(Vec<PointerValue<'ctx>>, Vec<inkwell::values::IntValue<'ctx>>)> {
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
        Ok((arg_ptrs, arg_lens))
    }

    /// Copy the first string into `buffer`, then append each remaining one.
    fn concat_copy_into(
        &mut self,
        buffer: PointerValue<'ctx>,
        strcpy_fn: inkwell::values::FunctionValue<'ctx>,
        arg_ptrs: &[PointerValue<'ctx>],
        arg_lens: &[inkwell::values::IntValue<'ctx>],
    ) -> Result<()> {
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
        Ok(())
    }

    pub(super) fn compile_to_string(&mut self, arguments: &[Expr]) -> Result<inkwell::values::BasicValueEnum<'ctx>> {
        if arguments.len() != 1 {
            return Err(HuziError::new_global("to_string() requires exactly 1 argument"));
        }

        let sprintf_fn = self.module.get_function("sprintf").unwrap();

        let arg = self.compile_expr(&arguments[0])?;

        // Booleans take a short fixed-size buffer with no format string.
        if let inkwell::values::BasicValueEnum::IntValue(iv) = arg {
            if iv.get_type().get_bit_width() == 1 {
                let s = self.build_bool_str(iv)?;
                let buffer = self.alloc_str_buffer(8)?;
                self.builder
                    .build_call(sprintf_fn, &[buffer.into(), s.into()], "sprintf")
                    .unwrap();
                return Ok(buffer.into());
            }
        }

        let (format_ptr, value) = self.to_string_format(arg)?;

        // Allocate buffer (large enough for any double formatting)
        let buffer = self.alloc_str_buffer(320)?;

        // Call sprintf
        self.builder
            .build_call(
                sprintf_fn,
                &[buffer.into(), format_ptr.into(), value.into()],
                "sprintf",
            )
            .unwrap();

        Ok(buffer.into())
    }

    /// Malloc a string buffer of the given size.
    fn alloc_str_buffer(&mut self, size: u64) -> Result<PointerValue<'ctx>> {
        let malloc_fn = self.module.get_function("malloc").unwrap();
        let buffer_size = self.context.i32_type().const_int(size, false);
        let buffer = self
            .builder
            .build_call(malloc_fn, &[buffer_size.into()], "str_buffer")
            .unwrap()
            .try_as_basic_value()
            .unwrap_left()
            .into_pointer_value();
        Ok(buffer)
    }

    /// Pick the printf-style format string for `arg` and promote the value
    /// to match C varargs conventions (chars to i32, floats to double).
    fn to_string_format(
        &mut self,
        arg: inkwell::values::BasicValueEnum<'ctx>,
    ) -> Result<(PointerValue<'ctx>, inkwell::values::BasicValueEnum<'ctx>)> {
        match arg {
            inkwell::values::BasicValueEnum::IntValue(iv) => {
                if iv.get_type().get_bit_width() == 64 {
                    let fmt = unsafe { self.builder.build_global_string("%ld", "fmt_i64").unwrap() };
                    Ok((fmt.as_pointer_value(), inkwell::values::BasicValueEnum::IntValue(iv)))
                } else if iv.get_type().get_bit_width() == 8 {
                    let fmt = unsafe { self.builder.build_global_string("%c", "fmt_c").unwrap() };
                    let promoted = self
                        .builder
                        .build_int_z_extend(iv, self.context.i32_type(), "char_promote")
                        .unwrap();
                    Ok((fmt.as_pointer_value(), inkwell::values::BasicValueEnum::IntValue(promoted)))
                } else {
                    let fmt = unsafe { self.builder.build_global_string("%d", "fmt_i32").unwrap() };
                    Ok((fmt.as_pointer_value(), inkwell::values::BasicValueEnum::IntValue(iv)))
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
                    Ok((fmt.as_pointer_value(), inkwell::values::BasicValueEnum::FloatValue(f64_val)))
                } else {
                    let fmt = unsafe { self.builder.build_global_string("%f", "fmt_f64").unwrap() };
                    Ok((fmt.as_pointer_value(), inkwell::values::BasicValueEnum::FloatValue(f64_val)))
                }
            }
            _ => {
                return Err(HuziError::new_global(
                    "to_string() requires a numeric argument",
                ))
            }
        }
    }

    // ==================== Struct Functions ====================


}
