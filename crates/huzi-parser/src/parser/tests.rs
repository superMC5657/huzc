use super::*;
use huzi_ast::*;
use huzi_lexer::Lexer;

fn parse(src: &str) -> Program {
    let tokens = Lexer::new(src.to_string())
        .tokenize()
        .unwrap_or_else(|e| panic!("unexpected lex error: {}", e));
    Parser::new(tokens)
        .parse()
        .unwrap_or_else(|e| panic!("unexpected parse error: {}", e))
}

#[test]
fn multiplication_binds_tighter_than_addition() {
    let program = parse("let r = 1 + 2 * 3");
    let Stmt::Let(let_stmt) = &program.statements[0].node else {
        panic!("expected a let statement");
    };
    let Some(Expr::Binary(add)) = &let_stmt.value else {
        panic!("expected a binary expression");
    };
    assert!(matches!(add.operator, BinOp::Add));
    let Expr::Literal(Literal::Int(1)) = &*add.left else {
        panic!("expected literal 1 as the left operand");
    };
    let Expr::Binary(mul) = &*add.right else {
        panic!("expected `2 * 3` as the right operand");
    };
    assert!(matches!(mul.operator, BinOp::Mul));
}

#[test]
fn let_mut_is_parsed_as_mutable() {
    let program = parse("let mut x = 1
let y = 2");
    let Stmt::Let(mutable) = &program.statements[0].node else {
        panic!("expected a let statement");
    };
    assert!(mutable.mutable);
    let Stmt::Let(immutable) = &program.statements[1].node else {
        panic!("expected a let statement");
    };
    assert!(!immutable.mutable);
}

#[test]
fn if_elif_else_structure() {
    let program = parse(
        "if x > 0 { let a = 1 } elif x < 0 { let b = 2 } else { let c = 3 }",
    );
    let Stmt::If(if_stmt) = &program.statements[0].node else {
        panic!("expected an if statement");
    };
    assert_eq!(if_stmt.then_branch.statements.len(), 1);
    assert_eq!(if_stmt.elif_branches.len(), 1);
    assert!(if_stmt.else_branch.is_some());
}

#[test]
fn import_parses_dotted_name() {
    let program = parse("import mods.helpers");
    let Stmt::Import(imp) = &program.statements[0].node else {
        panic!("expected an import statement");
    };
    assert_eq!(imp.name, "mods.helpers");
}

#[test]
fn parse_error_reports_real_position() {
    let tokens = Lexer::new("let x = ;".to_string())
        .tokenize()
        .expect("lexing must succeed");
    let err = Parser::new(tokens)
        .parse()
        .expect_err("`;` is not a valid expression");
    assert_eq!((err.line(), err.column()), (1, 9));
}

#[test]
fn statements_carry_source_span() {
    let program = parse("let x = 1\nlet y = 2");
    assert_eq!(program.statements[0].span.line, 1);
    assert_eq!(program.statements[0].span.column, 1);
    assert_eq!(program.statements[1].span.line, 2);
    assert_eq!(program.statements[1].span.column, 1);
}

/// `for x in arr` 应解析为 ForSource::Array。
#[test]
fn parses_for_in_array() {
    let program = parse("fn main() -> i32 {\n    for x in nums {\n        print(x)\n    }\n    return 0\n}");
    let Stmt::Fn(fn_stmt) = &program.statements[0].node else {
        panic!("expected fn main");
    };
    let mut found_array = false;
    for s in &fn_stmt.body.statements {
        if let Stmt::For(for_stmt) = &s.node {
            assert!(
                matches!(&for_stmt.source, ForSource::Array(_)),
                "for x in nums should parse as array iteration"
            );
            found_array = true;
        }
    }
    assert!(found_array, "for statement should be present");
}
