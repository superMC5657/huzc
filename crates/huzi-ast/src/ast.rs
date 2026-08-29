use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    I32,
    I64,
    U32,
    U64,
    F32,
    F64,
    Bool,
    Str,
    Char,
    Unit,
    Named(String),
    Array(Box<Type>, usize), // Array<ElementType, Size>
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::I32 => write!(f, "i32"),
            Type::I64 => write!(f, "i64"),
            Type::U32 => write!(f, "u32"),
            Type::U64 => write!(f, "u64"),
            Type::F32 => write!(f, "f32"),
            Type::F64 => write!(f, "f64"),
            Type::Bool => write!(f, "bool"),
            Type::Str => write!(f, "str"),
            Type::Char => write!(f, "char"),
            Type::Unit => write!(f, "()"),
            Type::Named(name) => write!(f, "{}", name),
            Type::Array(elem_type, size) => write!(f, "[{}; {}]", elem_type, size),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Program {
    pub statements: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Let(LetStmt),
    Struct(StructDef),
    Enum(EnumDef),
    Fn(FnStmt),
    Expr(ExprStmt),
    Return(ReturnStmt),
    Break,
    Continue,
    Block(Block),
    If(IfStmt),
    For(ForStmt),
    While(WhileStmt),
}

#[derive(Debug, Clone)]
pub struct LetStmt {
    pub name: String,
    pub mutable: bool,
    pub type_annotation: Option<Type>,
    pub value: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct FnStmt {
    pub name: String,
    pub params: Vec<FnParam>,
    pub return_type: Option<Type>,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct FnParam {
    pub name: String,
    pub param_type: Type,
}

#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<StructField>,
}

#[derive(Debug, Clone)]
pub struct StructField {
    pub name: String,
    pub field_type: Type,
}

#[derive(Debug, Clone)]
pub struct EnumDef {
    pub name: String,
    pub variants: Vec<EnumVariant>,
}

#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: String,
    /// Optional payload type: `Ok(i32)` has one, `Red` has none.
    pub payload: Option<Type>,
}

#[derive(Debug, Clone)]
pub struct ExprStmt {
    pub expr: Expr,
}

#[derive(Debug, Clone)]
pub struct ReturnStmt {
    pub value: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub statements: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub struct IfStmt {
    pub condition: Expr,
    pub then_branch: Block,
    pub elif_branches: Vec<(Expr, Block)>,
    pub else_branch: Option<Block>,
}

#[derive(Debug, Clone)]
pub struct ForStmt {
    pub var_name: String,
    pub start: Expr,
    pub end: Expr,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct WhileStmt {
    pub condition: Expr,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Literal(Literal),
    Ident(String),
    Binary(BinaryExpr),
    Unary(UnaryExpr),
    Call(CallExpr),
    Assign(AssignExpr),
    ArrayIndex(ArrayIndexExpr),
    ArrayLiteral(Vec<Expr>),
    If(IfExpr),
    FieldAccess(FieldAccessExpr),
    StructLiteral(StructLiteralExpr),
    EnumConstruct(EnumConstructExpr),
    Match(MatchExpr),
}

/// Enum variant construction: `Color::Red` or `Result::Ok(42)`
#[derive(Debug, Clone)]
pub struct EnumConstructExpr {
    pub enum_name: String,
    pub variant: String,
    pub args: Vec<Expr>,
}

/// `match scrutinee { pattern => body, ... }` used as an expression.
#[derive(Debug, Clone)]
pub struct MatchExpr {
    pub scrutinee: Box<Expr>,
    pub arms: Vec<MatchArm>,
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub enum Pattern {
    /// `Enum::Variant` or `Enum::Variant(binding)` — binds the payload to a
    /// variable inside the arm body.
    Variant {
        enum_name: String,
        variant: String,
        binding: Option<String>,
    },
    /// `_` — matches anything.
    Wildcard,
}

#[derive(Debug, Clone)]
pub struct FieldAccessExpr {
    pub base: Box<Expr>,
    pub field: String,
}

/// Struct instantiation: `Point { x: 1, y: 2 }`
#[derive(Debug, Clone)]
pub struct StructLiteralExpr {
    pub name: String,
    pub fields: Vec<(String, Expr)>,
}

/// If used as an expression: `let m = if cond { a } else { b }`
#[derive(Debug, Clone)]
pub struct IfExpr {
    pub condition: Box<Expr>,
    pub then_branch: Block,
    pub else_branch: Block,
}

#[derive(Debug, Clone)]
pub struct ArrayIndexExpr {
    pub array: Box<Expr>,
    pub index: Box<Expr>,
}

#[derive(Debug, Clone)]
pub enum Literal {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Char(char),
}

#[derive(Debug, Clone)]
pub struct BinaryExpr {
    pub left: Box<Expr>,
    pub operator: BinOp,
    pub right: Box<Expr>,
}

#[derive(Debug, Clone)]
pub struct UnaryExpr {
    pub operator: UnOp,
    pub operand: Box<Expr>,
}

#[derive(Debug, Clone)]
pub struct CallExpr {
    pub callee: Box<Expr>,
    pub arguments: Vec<Expr>,
}

#[derive(Debug, Clone)]
pub struct AssignExpr {
    pub target: Box<Expr>,
    pub value: Box<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Neq,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnOp {
    Neg,
    Not,
}
