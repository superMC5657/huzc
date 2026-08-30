//! 文件 I/O 内置函数:`read_file(path)` / `write_file(path, content)`。
//!
//! `read_file` 一次性读入整个文件(基于 fseek/ftell 定长,≤2GB),
//! 返回以 `\0` 结尾的堆上字符串;打开失败返回空串。
//! `write_file` 以文本模式整体写入,返回是否成功(true/false)。

use huzi_ast::Expr;
use huzi_error::{HuziError, Result};
use inkwell::values::BasicValueEnum;

use super::CodeGen;

impl<'ctx> CodeGen<'ctx> {
    /// `read_file(path: str) -> str`:读入整个文件;失败返回空串。
    pub(super) fn compile_read_file(
        &mut self,
        arguments: &[Expr],
    ) -> Result<BasicValueEnum<'ctx>> {
        if arguments.len() != 1 {
            return Err(HuziError::new_global(
                "read_file() requires exactly 1 argument (path)",
            ));
        }
        let path = self.compile_str_arg(&arguments[0], "read_file")?;

        let fopen_fn = self.module.get_function("fopen").expect("fopen in prelude");
        let fseek_fn = self.module.get_function("fseek").expect("fseek in prelude");
        let ftell_fn = self.module.get_function("ftell").expect("ftell in prelude");
        let fread_fn = self.module.get_function("fread").expect("fread in prelude");
        let fclose_fn = self.module.get_function("fclose").expect("fclose in prelude");
        let malloc_fn = self.module.get_function("malloc").expect("malloc in prelude");

        let function = self.current_function()?;
        let ok_block = self.context.append_basic_block(function, "rf_ok");
        let fail_block = self.context.append_basic_block(function, "rf_fail");
        let end_block = self.context.append_basic_block(function, "rf_end");

        let mode = self.cstr_const("rb");
        let call = self.builder.build_call(
            fopen_fn,
            &[path.into(), mode.into()],
            "rf_file",
        ).unwrap();
        let file = call.try_as_basic_value().unwrap_left().into_pointer_value();
        let result_ptr = self.build_alloca(self.context.ptr_type(inkwell::AddressSpace::default()).into(), "rf_res")?;

        let is_null = self
            .builder
            .build_is_null(file, "rf_is_null")
            .unwrap();
        self.builder
            .build_conditional_branch(is_null, fail_block, ok_block)
            .unwrap();

        // 打开失败:返回空串。
        self.builder.position_at_end(fail_block);
        let empty = unsafe { self.builder.build_global_string("", "huzi_empty_str").unwrap() };
        self.builder
            .build_store(result_ptr, empty.as_pointer_value())
            .unwrap();
        self.builder
            .build_unconditional_branch(end_block)
            .unwrap();

        // 打开成功:定位文件尾取长度,回到开头整块读入,补 \0。
        self.builder.position_at_end(ok_block);
        let zero32 = self.context.i32_type().const_int(0, false);
        let seek_end = self.context.i32_type().const_int(2, false);
        self.emit_void_call(fseek_fn, &[
            file.into(),
            self.context.i64_type().const_int(0, false).into(),
            seek_end.into(),
        ], "rf_seek_end");
        let size = self
            .builder
            .build_call(ftell_fn, &[file.into()], "rf_size")
            .unwrap()
            .try_as_basic_value()
            .unwrap_left()
            .into_int_value();
        self.emit_void_call(fseek_fn, &[
            file.into(),
            self.context.i64_type().const_int(0, false).into(),
            zero32.into(),
        ], "rf_seek_set");

        let size_plus_one = self
            .builder
            .build_int_add(size, self.context.i32_type().const_int(1, false), "rf_buf_len")
            .unwrap();
        let buf = self
            .builder
            .build_call(malloc_fn, &[size_plus_one.into()], "rf_buf")
            .unwrap()
            .try_as_basic_value()
            .unwrap_left()
            .into_pointer_value();
        let size64 = self
            .builder
            .build_int_s_extend(size, self.context.i64_type(), "rf_size64")
            .unwrap();
        let one64 = self.context.i64_type().const_int(1, false);
        let n = self
            .builder
            .build_call(fread_fn, &[buf.into(), one64.into(), size64.into(), file.into()], "rf_n")
            .unwrap()
            .try_as_basic_value()
            .unwrap_left()
            .into_int_value();
        let term = unsafe {
            self.builder
                .build_gep(self.context.i8_type(), buf, &[n], "rf_term")
                .unwrap()
        };
        self.builder
            .build_store(term, self.context.i8_type().const_int(0, false))
            .unwrap();
        self.emit_void_call(fclose_fn, &[file.into()], "rf_close");
        self.builder
            .build_store(result_ptr, buf)
            .unwrap();
        self.builder
            .build_unconditional_branch(end_block)
            .unwrap();

        self.builder.position_at_end(end_block);
        let result = self
            .builder
            .build_load(self.context.ptr_type(inkwell::AddressSpace::default()), result_ptr, "rf_load")
            .unwrap();
        Ok(result)
    }

    /// `write_file(path: str, content: str) -> bool`:整体写入文本文件。
    pub(super) fn compile_write_file(
        &mut self,
        arguments: &[Expr],
    ) -> Result<BasicValueEnum<'ctx>> {
        if arguments.len() != 2 {
            return Err(HuziError::new_global(
                "write_file() requires exactly 2 arguments (path, content)",
            ));
        }
        let path = self.compile_str_arg(&arguments[0], "write_file")?;
        let content = self.compile_str_arg(&arguments[1], "write_file")?;

        let fopen_fn = self.module.get_function("fopen").expect("fopen in prelude");
        let fwrite_fn = self.module.get_function("fwrite").expect("fwrite in prelude");
        let fclose_fn = self.module.get_function("fclose").expect("fclose in prelude");
        let strlen_fn = self.module.get_function("strlen").expect("strlen in prelude");

        let function = self.current_function()?;
        let ok_block = self.context.append_basic_block(function, "wf_ok");
        let fail_block = self.context.append_basic_block(function, "wf_fail");
        let end_block = self.context.append_basic_block(function, "wf_end");

        let mode = self.cstr_const("wb");
        let call = self.builder.build_call(
            fopen_fn,
            &[path.into(), mode.into()],
            "wf_file",
        ).unwrap();
        let file = call.try_as_basic_value().unwrap_left().into_pointer_value();
        let result_ptr = self.build_alloca(self.context.bool_type().into(), "wf_res")?;

        let is_null = self.builder.build_is_null(file, "wf_is_null").unwrap();
        self.builder
            .build_conditional_branch(is_null, fail_block, ok_block)
            .unwrap();

        // 打开失败:返回 false。
        self.builder.position_at_end(fail_block);
        self.builder
            .build_store(result_ptr, self.context.bool_type().const_int(0, false))
            .unwrap();
        self.builder
            .build_unconditional_branch(end_block)
            .unwrap();

        // 打开成功:全量写入,按写入字节数判定成败。
        self.builder.position_at_end(ok_block);
        let one64 = self.context.i64_type().const_int(1, false);
        let len = self
            .builder
            .build_call(strlen_fn, &[content.into()], "wf_len")
            .unwrap()
            .try_as_basic_value()
            .unwrap_left()
            .into_int_value();
        let len64 = self
            .builder
            .build_int_z_extend(len, self.context.i64_type(), "wf_len64")
            .unwrap();
        let written = self
            .builder
            .build_call(fwrite_fn, &[content.into(), one64.into(), len64.into(), file.into()], "wf_n")
            .unwrap()
            .try_as_basic_value()
            .unwrap_left()
            .into_int_value();
        let all_written = self
            .builder
            .build_int_compare(inkwell::IntPredicate::EQ, written, len64, "wf_all")
            .unwrap();
        self.emit_void_call(fclose_fn, &[file.into()], "wf_close");
        self.builder
            .build_store(result_ptr, all_written)
            .unwrap();
        self.builder
            .build_unconditional_branch(end_block)
            .unwrap();

        self.builder.position_at_end(end_block);
        let result = self
            .builder
            .build_load(self.context.bool_type(), result_ptr, "wf_load")
            .unwrap();
        Ok(result)
    }

    /// 编译一个求值为字符串(i8*)的参数。
    fn compile_str_arg(&mut self, expr: &Expr, name: &str) -> Result<inkwell::values::PointerValue<'ctx>> {
        let value = self.compile_expr(expr)?;
        match value {
            BasicValueEnum::PointerValue(p) => Ok(p),
            _ => Err(HuziError::new_global(format!(
                "{}() argument must be a string",
                name
            ))),
        }
    }

    /// 生成一个不使用返回值的调用(void 函数)。
    fn emit_void_call(
        &mut self,
        function: inkwell::values::FunctionValue<'ctx>,
        args: &[inkwell::values::BasicMetadataValueEnum<'ctx>],
        name: &str,
    ) {
        self.builder.build_call(function, args, name).unwrap();
    }

    /// 取(或惰性创建)模块级 C 字符串常量。
    fn cstr_const(&mut self, s: &str) -> inkwell::values::PointerValue<'ctx> {
        let global = unsafe { self.builder.build_global_string(s, "huzi_cstr").unwrap() };
        global.as_pointer_value()
    }
}
