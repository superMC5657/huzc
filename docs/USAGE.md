# Huzi 编程语言 & Huzc 编译器用户指南

## 简介

Huzi 是一种简洁的编译型编程语言，语法类似 Python，编译后生成高效的可执行文件。Huzc 是 Huzi 语言的编译器，使用 Rust 开发，LLVM-18 作为后端。

## 环境要求

- Rust 1.70+
- LLVM 18
- Windows 10/11 (x86_64)
- clang (用于链接)

## 快速开始

### 1. 构建编译器

```bash
cargo build --release
```

### 2. 编译 Huzi 程序

```bash
# 基本用法
cargo run --release --bin huzc -- --input <源文件.hz> -o <输出.exe>

# 示例
cargo run --release --bin huzc -- --input examples/hello.hz -o hello.exe
```

### 3. 运行程序

```bash
./hello.exe
```

## 编译器选项

| 选项 | 说明 | 示例 |
|------|------|------|
| `--input <file>` | 输入的 .hz 源文件 | `--input hello.hz` |
| `-o <file>` | 输出的可执行文件 | `-o hello.exe` |
| `--emit-llvm` | 输出 LLVM IR (.ll 文件) | `--emit-llvm -o hello.ll` |
| `--only-compile` | 只编译到目标文件，不链接 | `--only-compile -o hello.obj` |

## 语言语法

### 1. 变量声明

```python
# 不可变变量
let x = 10

# 可变变量
let mut y = 20

# 带类型注解
let z: i32 = 30
```

### 2. 函数定义

```python
# 有返回值
fn add(a: i32, b: i32) -> i32 {
    return a + b
}

# 无返回值 (返回 i32)
fn greet() -> i32 {
    print("Hello!")
    return 0
}
```

### 3. 控制流

```python
# 条件判断
if x > 10 {
    print("big")
} elif x > 5 {
    print("medium")
} else {
    print("small")
}

# 循环 (范围: start..end)
for i in 0..10 {
    print(i)
}

# 条件循环
while x > 0 {
    x = x - 1
}
```

### 4. 内置函数

```python
# 打印字符串 (目前仅支持)
print("Hello")      # 打印字符串
```

> **注意**: 目前 `print` 仅支持字符串类型。整数打印功能开发中。

## 示例程序

### Hello World

```python
fn main() -> i32 {
    print("Hello, World!")
    return 0
}
```

### 阶乘计算

```python
fn factorial(n: i32) -> i32 {
    if n <= 1 {
        return 1
    }
    return n * factorial(n - 1)
}

fn main() -> i32 {
    let result = factorial(5)
    print("Factorial(5) = ")
    print(result)
    return 0
}
```

### 斐波那契数列

```python
fn main() -> i32 {
    let a = 0
    let b = 1
    for i in 0..10 {
        let temp = a + b
        a = b
        b = temp
    }
    print("Fibonacci(10) = ")
    print(b)
    return 0
}
```

### 循环求和

```python
fn main() -> i32 {
    let sum = 0
    for i in 1..101 {
        sum = sum + i
    }
    print("Sum of 1..100 = ")
    print(sum)
    return 0
}
```

## 支持的类型

| 类型 | 说明 | 示例 |
|------|------|------|
| `i32` | 32位整数 | `let x: i32 = 42` |
| `i64` | 64位整数 | `let x: i64 = 42` |
| `f32` | 32位浮点数 | `let x: f32 = 3.14` |
| `f64` | 64位浮点数 | `let x: f64 = 3.14` |
| `bool` | 布尔值 | `let x: bool = true` |
| `str` | 字符串 | `let x: str = "hello"` |
| `char` | 字符 | `let x: char = 'a'` |

## 运算符

### 算术运算符
```python
+   # 加法
-   # 减法
*   # 乘法
/   # 除法
%   # 取模
```

### 比较运算符
```python
==  # 等于
!=  # 不等于
<   # 小于
>   # 大于
<=  # 小于等于
>=  # 大于等于
```

### 逻辑运算符
```python
&&  # 逻辑与
||  # 逻辑或
!   # 逻辑非
```

## 关键字

| 关键字 | 说明 |
|--------|------|
| `fn` | 函数定义 |
| `let` | 变量声明 |
| `mut` | 可变变量 |
| `if` | 条件判断 |
| `elif` | 多条件分支 |
| `else` | 否则分支 |
| `for` | 循环 |
| `while` | 条件循环 |
| `return` | 返回值 |
| `in` | 循环范围 |
| `print` | 打印输出 |

## 编译流程

```
.hz 源文件
    ↓
[1/5] 词法分析 (Lexer)
    ↓
[2/5] 语法分析 (Parser)
    ↓
[3/5] 代码生成 (CodeGen) → LLVM IR
    ↓
[4/5] 验证 (Verify)
    ↓
[5/5] 链接 (Linker) → .exe
```

## 项目结构

```
huzc/
├── examples/           # 示例程序
│   ├── hello.hz
│   ├── fib.hz
│   ├── loop_sum.hz
│   └── fact.hz
├── crates/
│   ├── huzc/         # 编译器入口
│   ├── huzi-lexer/   # 词法分析器
│   ├── huzi-parser/  # 语法分析器
│   ├── huzi-codegen/ # LLVM 代码生成
│   ├── huzi-ast/     # AST 定义
│   └── huzi-error/   # 错误处理
└── TECHNICAL.md      # 技术文档
```

## 常见问题

### Q: 编译报错 "Verification failed"
A: 这是 LLVM 验证警告，编译器会继续生成可执行文件。如果生成的程序无法运行，请检查代码逻辑。

### Q: 如何调试生成的 LLVM IR
A: 使用 `--emit-llvm` 参数生成 .ll 文件，然后用 `llvm-dis` 反汇编查看：
```bash
huzc --input hello.hz --emit-llvm -o hello.llvm
llvm-dis hello.llvm
cat hello.ll
```

### Q: 支持递归函数吗
A: 支持。详见示例 `fact.hz`。

## 后续计划

- [ ] 完善标准库
- [ ] 添加更多优化选项
- [ ] 支持更多平台 (Linux, macOS)
- [ ] 错误消息优化
- [ ] 调试信息生成

---

**祝您使用愉快！**
