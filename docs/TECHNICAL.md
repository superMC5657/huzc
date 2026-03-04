# Huzi 编程语言技术开发文档

## 1. 项目概述

- **语言名称**: Huzi
- **编译器名称**: Huzc
- **开发语言**: Rust
- **后端**: LLVM-18 (inkwell 0.5.0)
- **语言类型**: 编译型语言
- **语言风格**: 类似 Python 的简洁语法，类似 Go 的编译型语言

## 2. 项目架构

### 2.1 Workspace 结构

```
huzc/
├── Cargo.toml           # Workspace 根配置
├── crates/
│   ├── huzc/            # 主编译器 crate
│   ├── huzi-ast/        # 抽象语法树定义
│   ├── huzi-lexer/      # 词法分析器
│   ├── huzi-parser/     # 语法分析器
│   ├── huzi-codegen/    # LLVM 代码生成
│   └── huzi-error/      # 错误处理模块
└── docs/
```

### 2.2 编译流程

```
源代码 → 词法分析 → 语法分析 → AST → 语义分析 → LLVM IR → 优化 → 目标文件 → 可执行文件
```

## 3. 语言特性 (基础版)

### 3.1 基础类型
- `i32`, `i64`, `u32`, `u64`, `f32`, `f64` - 整数和浮点数
- `bool` - 布尔值
- `str` - 字符串
- `char` - 字符

### 3.2 关键字
```
fn      - 函数定义
let     - 变量声明
mut     - 可变变量
if      - 条件判断
else    - 条件分支
elif    - 多条件分支
for     - 循环
while   - 条件循环
return  - 返回值
true    - 真
false   - 假
print   - 打印
```

### 3.3 语法示例

```python
# 变量声明
let x: i32 = 10
let mut y = 20

# 函数定义
fn add(a: i32, b: i32) -> i32 {
    return a + b
}

# 控制流
if x > 5 {
    print("big")
} elif x > 2 {
    print("medium")
} else {
    print("small")
}

# 循环
for i in 0..10 {
    print(i)
}
```

## 4. 模块设计

### 4.1 huzc (主 crate)
- 命令行入口
- 编译流程协调
- 文件系统操作

### 4.2 huzi-ast
- AST 节点定义
- AST 遍历工具

### 4.3 huzi-lexer
- Token 定义
- 词法分析器实现

### 4.4 huzi-parser
- 语法规则定义
- 解析器实现 (递归下降)

### 4.5 huzi-codegen
- LLVM 上下文管理
- 代码生成
- 目标代码发射

### 4.6 huzi-error
- 错误类型定义
- 错误报告格式化

## 5. 依赖版本

```toml
inkwell = { version = "0.5.0", features = ["llvm18-0"] }
```

## 6. 开发计划

### Phase 1: 基础设施
- [ ] 创建 workspace 结构
- [ ] 配置 inkwell 依赖
- [ ] 实现错误处理模块

### Phase 2: 前端
- [ ] 实现词法分析器
- [ ] 实现语法分析器
- [ ] 构建 AST

### Phase 3: 后端
- [ ] 集成 LLVM
- [ ] 实现代码生成
- [ ] 支持可执行文件输出

### Phase 4: 语言特性
- [ ] 变量和类型
- [ ] 函数调用
- [ ] 控制流
- [ ] 基础标准库 (print)
