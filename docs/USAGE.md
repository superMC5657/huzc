# Huzi 编程语言 & Huzc 编译器用户指南

## 简介

Huzi 是一种简洁的编译型编程语言，语法类似 Python，编译后生成高效的可执行文件。Huzc 是 Huzi 语言的编译器，使用 Rust 开发，LLVM-18 作为后端。

## 环境要求

- Rust 1.70+
- LLVM 18
- Windows 10/11 (x86_64) / Linux / macOS
- clang 或 lld-link (用于链接)

## 快速开始

### 1. 构建编译器

```bash
cargo build --release
```

### 2. 编译 Huzi 程序

```bash
# 基本用法
cargo run --release --bin huzc -- --input <源文件.hz> -o <输出名称>

# 示例 - 编译到当前目录
cargo run --release --bin huzc -- --input examples/hello.hz -o hello

# 示例 - 编译到子目录
cargo run --release --bin huzc -- --input examples/hello.hz -o out/hello

# 示例 - 输出 LLVM IR
cargo run --release --bin huzc -- --input examples/hello.hz --emit-llvm -o out/hello
```

### 3. 运行程序

```bash
# Windows
./hello.exe

# Linux/macOS
./hello
```

## 编译器选项

| 选项 | 说明 | 示例 |
|------|------|------|
| `--input <file>` | 输入的 .hz 源文件 | `--input hello.hz` |
| `-o <name>` | 输出文件基础名 (自动添加平台扩展名) | `-o hello` → `hello.exe` (Windows) |
| `--emit-llvm` | 输出 LLVM IR (.ll 文件) | `--emit-llvm -o hello` |
| `--only-compile` | 只编译到目标文件，不链接 | `--only-compile -o hello` |

### 输出文件说明

- **Windows**: 输出 `hello.exe`，中间文件 `hello.ll`、`hello.obj`
- **Linux/macOS**: 输出 `hello`，中间文件 `hello.ll`、`hello.o`
- 中间文件在编译完成后自动清理
- 使用 `--emit-llvm` 时保留 `.ll` 文件

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

# 无返回值
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

# 循环 (范围：start..end)
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
# 打印 - 支持所有基本类型
print("Hello")           # 字符串
print(42)                # 整数
print(3.14)              # 浮点数
print(true)              # 布尔值
print("x =", x)          # 多参数
```

## 示例程序

### Hello World

```python
fn main() -> i32 {
    print("Hello, World!")
    return 0
}
```

### 数组使用

```python
fn main() -> i32 {
    # 数组字面量
    let arr = [1, 2, 3, 4, 5]
    
    # 访问数组元素
    print("First:", arr[0])
    print("Third:", arr[2])
    
    # 数组求和
    let sum = 0
    for i in 0..5 {
        sum = sum + arr[i]
    }
    print("Sum =", sum)
    
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
    print("Factorial(5) =", result)
    return 0
}
```

### 数学函数

```python
fn main() -> i32 {
    # 平方根
    let r = sqrt(16.0)
    print("sqrt(16) =", r)
    
    # 幂运算
    let p = pow(2.0, 10.0)
    print("2^10 =", p)
    
    # 三角函数
    let s = sin(3.14159 / 2)
    print("sin(π/2) =", s)
    
    # 绝对值
    let a = abs(-42)
    print("abs(-42) =", a)
    
    return 0
}
```

### 字符串操作

```python
fn main() -> i32 {
    # 字符串长度
    let s = "hello"
    print("len:", len(s))
    
    # 字符串拼接
    let a = "Hello, "
    let b = "World!"
    let c = concat(a, b)
    print(c)
    
    # 数值转字符串
    let num = 42
    let str = to_string(num)
    print("num as string:", str)
    
    return 0
}
```

### 输入函数

```python
fn main() -> i32 {
    # 读取字符串
    let name = read_line()
    print("Hello,", name)
    
    # 读取整数
    let age = read_int()
    print("Age:", age)
    
    # 读取浮点数
    let height = read_float()
    print("Height:", height)
    
    return 0
}
```

## 支持的类型

| 类型 | 说明 | 示例 |
|------|------|------|
| `i32` | 32 位整数 | `let x: i32 = 42` |
| `i64` | 64 位整数 | `let x: i64 = 42` |
| `f32` | 32 位浮点数 | `let x: f32 = 3.14` |
| `f64` | 64 位浮点数 | `let x: f64 = 3.14` |
| `bool` | 布尔值 | `let x: bool = true` |
| `str` | 字符串 | `let x: str = "hello"` |
| `char` | 字符 | `let x: char = 'a'` |
| `[T; N]` | 数组 | `let arr: [i32; 5] = [1, 2, 3, 4, 5]` |

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

## 标准库函数

### 输入输出
| 函数 | 说明 | 示例 |
|------|------|------|
| `print(...)` | 打印输出 | `print("x =", x)` |
| `read_line()` | 读取一行字符串 | `let s = read_line()` |
| `read_int()` | 读取整数 | `let n = read_int()` |
| `read_float()` | 读取浮点数 | `let f = read_float()` |

### 字符串
| 函数 | 说明 | 示例 |
|------|------|------|
| `len(s)` | 获取字符串长度 | `len("hello")` → 5 |
| `concat(a, b)` | 字符串拼接 | `concat("a", "b")` → "ab" |
| `to_string(x)` | 数值转字符串 | `to_string(42)` → "42" |

### 数学
| 函数 | 说明 | 示例 |
|------|------|------|
| `abs(x)` | 绝对值 | `abs(-5)` → 5.0 |
| `sqrt(x)` | 平方根 | `sqrt(16.0)` → 4.0 |
| `pow(x, n)` | 幂运算 | `pow(2.0, 3.0)` → 8.0 |
| `sin(x)` | 正弦 | `sin(0)` → 0.0 |
| `cos(x)` | 余弦 | `cos(0)` → 1.0 |

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
[5/5] 链接 (Linker) → 可执行文件
```

## 项目结构

```
huzc/
├── examples/           # 示例程序
│   ├── hello.hz
│   ├── array.hz        # 数组示例
│   ├── fact.hz         # 阶乘示例
│   └── ...
├── crates/
│   ├── huzc/           # 编译器入口
│   ├── huzi-lexer/     # 词法分析器
│   ├── huzi-parser/    # 语法分析器
│   ├── huzi-codegen/   # LLVM 代码生成
│   ├── huzi-ast/       # AST 定义
│   └── huzi-error/     # 错误处理
└── docs/
    ├── USAGE.md        # 用户指南
    └── TECHNICAL.md    # 技术文档
```

## 常见问题

### Q: 编译报错 "Verification failed"
A: 这是 LLVM 验证警告，编译器会继续生成可执行文件。如果生成的程序无法运行，请检查代码逻辑。

### Q: 如何调试生成的 LLVM IR
A: 使用 `--emit-llvm` 参数生成 .ll 文件查看：
```bash
huzc --input hello.hz --emit-llvm -o hello
cat hello.ll
```

### Q: 支持递归函数吗
A: 支持。详见示例 `fact.hz`。

### Q: 输出文件在哪里
A: 中间文件 (`.ll` 和 `.obj/.o`) 与输出文件在同一目录，编译完成后自动清理。

### Q: 如何指定输出目录
A: 使用 `-o` 指定路径即可：
```bash
huzc --input src/main.hz -o build/myapp
# 生成 build/myapp.exe (Windows) 或 build/myapp (Linux/macOS)
```

## 后续计划

- [ ] 结构体支持
- [ ] 枚举和 match 表达式
- [ ] 类型验证和推导增强
- [ ] 更多标准库函数
- [ ] 模块系统
- [ ] 调试信息生成

---

**祝您使用愉快！**
