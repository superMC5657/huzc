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
| `--release` (`-r`) | Release 模式：生成代码前先用 `opt -O2` 优化 LLVM IR，运行速度显著更快；编译过程不打印任何日志（错误仍输出到 stderr）。默认 dev 模式不做 IR 优化并打印编译进度 | `huzc --input main.hz -o main --release` |
| `--opt-level <0-3>` | LLVM 优化级别,覆盖 `--release` 的默认级别 2;`--opt-level 0` 等价于 dev 模式 | `huzc --input main.hz -o main --opt-level 3` |

### 输出文件说明

- **Windows**: 输出 `hello.exe`，中间文件 `hello.ll`、`hello.obj`
- **Linux/macOS**: 输出 `hello`，中间文件 `hello.ll`、`hello.o`
- 中间文件在编译完成后自动清理

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

### 5. 结构体

```python
# 定义结构体
struct Point {
    x: i32,
    y: i32,
}

# 实例化（必须提供全部字段）
let p = Point { x: 3, y: 4 }

# 字段访问
print(p.x, p.y)

# 字段赋值（变量需要 let mut 声明）
let mut q = Point { x: 0, y: 0 }
q.x = 10

# 嵌套字段访问
struct Rect {
    origin: Point,
    w: i32,
    h: i32,
}
let rect = Rect { origin: p, w: 100, h: 50 }
print(rect.origin.x)

# 结构体作为函数参数/返回值（按值传递，赋值时逐字段拷贝）
fn sum_points(a: Point, b: Point) -> i32 {
    return a.x + b.x + a.y + b.y
}
fn make_point(v: i32) -> Point {
    return Point { x: v, y: v * 2 }
}

# 数组中的结构体、结构体的数组字段
let points = [p, q]
points[0].x = 99      # points 需要 let mut
struct Data {
    nums: [i32; 3],
    total: i32,
}
let data = Data { nums: [1, 2, 3], total: 6 }
print(data.nums[2], len(data.nums))
```

限制：结构体不支持自引用/相互嵌套的值循环（`struct A { b: B }` + `struct B { a: A }` 会报编译错误）；`print` 不直接支持整个结构体，需要逐字段打印。

### 6. 枚举与 match

```python
# 简单枚举（值为判别码，可 == 比较，print 打印为整数）
enum Color {
    Red,
    Green,
    Blue,
}

# 带数据的枚举（每个变体至多一个 payload）
enum Shape {
    Circle(f64),
    Rect(f64),
    Point2D,
}

# 构造变体：Enum::Variant 或 Enum::Variant(payload)
let c = Color::Green
let s = Shape::Circle(2.0)

# match 作为表达式，每个分支产出值
fn area(s: Shape) -> f64 {
    return match s {
        Shape::Circle(r) => 3.14159 * r * r,   # r 绑定 payload
        Shape::Rect(w) => w * w,
        Shape::Point2D => 0.0,
        _ => 0.0,                              # 必须有 wildcard 兜底
    }
}

print(area(s))          # 12.56636
print(c == Color::Red)  # false
```

限制：每个变体至多一个 payload；match 必须包含 `_` 分支（不做穷尽性检查）；带数据的枚举不支持 `==` 比较；`print` 简单枚举输出的是判别码整数。

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

## 模块系统

### import 语句

```huzi
import math              # 内置模块
import mods.helpers      # 文件模块:解析为 mods/helpers.hz
```

- **文件模块**:相对导入文件所在目录查找 `<路径>.hz`(其次当前工作目录);点分名对应子路径。
- 模块文件只允许 `fn`/`struct`/`enum` 定义与 `import`,不允许顶层级语句。
- 导入后通过**限定名**使用:`helpers::add(1, 2)`、`math::sqrt(4.0)`;文件模块的
  `struct`/`enum` 注册为全局名,直接按原名使用。
- 同一模块只编译一次(按解析路径去重),循环导入会在首次访问处截断。
- 模块内函数可调用同模块其它函数与内置函数,不能调用主程序里的函数。

### 内置模块

| 模块 | 说明 |
|------|------|
| `math` | 数学函数的限定形式:`math::sqrt(x)`、`math::pow(x, n)`、`math::sin(x)` 等,与内置同名函数等价 |

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
| `import` | 导入模块 |
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
[可选] IR 优化 (仅 --release 模式: opt -O2)
    ↓
[5/5] 链接 (Linker) → 可执行文件
```

> 说明：`cargo build` 的 debug/release 只影响 huzc 编译器自身的编译速度，
> 不影响它生成的程序。想让生成的程序更快，请使用 `--release` 编译选项。

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

### Q: 如何查看生成的 LLVM IR
A: 中间文件 `.ll` 在编译完成后会自动清理。如需查看，可临时修改代码保留中间文件。

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
