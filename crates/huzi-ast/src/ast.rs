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
    Fn(FnStmt),
    Expr(ExprStmt),
    Return(ReturnStmt),
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
