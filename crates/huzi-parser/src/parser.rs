use huzi_ast::*;
use huzi_error::HuziError;
use huzi_error::Result;
use huzi_lexer::SpannedToken;
use huzi_lexer::Token;

pub struct Parser {
    tokens: Vec<SpannedToken>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<SpannedToken>) -> Self {
        Self { tokens, pos: 0 }
    }

    pub fn parse(&mut self) -> Result<Program> {
        let mut statements = Vec::new();

        while !self.is_at_end() {
            statements.push(self.parse_statement()?);
        }

        Ok(Program { statements })
    }

    fn parse_statement(&mut self) -> Result<Stmt> {
        if self.check_keyword(&[Token::Let]) {
            self.parse_let_statement()
        } else if self.check_keyword(&[Token::Struct]) {
            self.parse_struct_statement()
        } else if self.check_keyword(&[Token::Enum]) {
            self.parse_enum_statement()
        } else if self.check_keyword(&[Token::Fn]) {
            self.parse_fn_statement()
        } else if self.check_keyword(&[Token::Return]) {
            self.parse_return_statement()
        } else if self.check_keyword(&[Token::Break]) {
            self.advance();
            Ok(Stmt::Break)
        } else if self.check_keyword(&[Token::Continue]) {
            self.advance();
            Ok(Stmt::Continue)
        } else if self.check_keyword(&[Token::If]) {
            self.parse_if_statement()
        } else if self.check_keyword(&[Token::For]) {
            self.parse_for_statement()
        } else if self.check_keyword(&[Token::While]) {
            self.parse_while_statement()
        } else if self.check(&Token::LBrace) {
            Ok(Stmt::Block(self.parse_block()?))
        } else {
            let expr = self.parse_expression()?;
            Ok(Stmt::Expr(ExprStmt { expr }))
        }
    }

    fn parse_let_statement(&mut self) -> Result<Stmt> {
        self.advance();

        // `let mut name` or `let name`
        let mutable = self.check(&Token::Mut);
        if mutable {
            self.advance();
        }

        let name = self.expect_ident("Expected variable name")?;

        let type_annotation = if self.check(&Token::Colon) {
            self.advance();
            Some(self.parse_type()?)
        } else {
            None
        };

        let value = if self.check(&Token::Equal) {
            self.advance();
            Some(self.parse_expression()?)
        } else {
            None
        };

        Ok(Stmt::Let(LetStmt {
            name,
            mutable,
            type_annotation,
            value,
        }))
    }

    fn parse_struct_statement(&mut self) -> Result<Stmt> {
        self.advance();

        let name = self.expect_ident("Expected struct name")?;

        self.expect(&Token::LBrace, "Expected '{' after struct name")?;

        let mut fields = Vec::new();
        while !self.check(&Token::RBrace) && !self.is_at_end() {
            let field_name = self.expect_ident("Expected field name")?;

            self.expect(&Token::Colon, "Expected ':' after field name")?;
            let field_type = self.parse_type()?;

            fields.push(StructField {
                name: field_name,
                field_type,
            });

            if self.check(&Token::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        self.expect(&Token::RBrace, "Expected '}' after struct fields")?;

        Ok(Stmt::Struct(StructDef { name, fields }))
    }

    fn parse_enum_statement(&mut self) -> Result<Stmt> {
        self.advance();

        let name = self.expect_ident("Expected enum name")?;

        self.expect(&Token::LBrace, "Expected '{' after enum name")?;

        let mut variants = Vec::new();
        while !self.check(&Token::RBrace) && !self.is_at_end() {
            let variant_name = self.expect_ident("Expected variant name")?;

            let payload = if self.check(&Token::LParen) {
                self.advance();
                let payload_type = self.parse_type()?;
                if self.check(&Token::Comma) {
                    return Err(HuziError::new(
                        "Multiple payload values per variant are not supported",
                        self.current_line(),
                        self.current_col(),
                    ));
                }
                self.expect(&Token::RParen, "Expected ')' after variant payload type")?;
                Some(payload_type)
            } else {
                None
            };

            variants.push(EnumVariant {
                name: variant_name,
                payload,
            });

            if self.check(&Token::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        self.expect(&Token::RBrace, "Expected '}' after enum variants")?;

        Ok(Stmt::Enum(EnumDef { name, variants }))
    }

    fn parse_fn_statement(&mut self) -> Result<Stmt> {
        self.advance();

        let name = self.expect_ident("Expected function name")?;

        self.expect(&Token::LParen, "Expected '(' after function name")?;

        let mut params = Vec::new();
        while !self.check(&Token::RParen) {
            let param_name = self.expect_ident("Expected parameter name")?;

            self.expect(&Token::Colon, "Expected ':' after parameter name")?;
            let param_type = self.parse_type()?;

            params.push(FnParam {
                name: param_name,
                param_type,
            });

            if self.check(&Token::Comma) {
                self.advance();
            }
        }
        self.expect(&Token::RParen, "Expected ')' after parameters")?;

        let return_type = if self.check(&Token::Arrow) {
            self.advance();
            Some(self.parse_type()?)
        } else {
            None
        };

        let body = self.parse_block()?;

        Ok(Stmt::Fn(FnStmt {
            name,
            params,
            return_type,
            body,
        }))
    }

    fn parse_return_statement(&mut self) -> Result<Stmt> {
        self.advance();

        let value = if self.is_expr_start() {
            Some(self.parse_expression()?)
        } else {
            None
        };

        Ok(Stmt::Return(ReturnStmt { value }))
    }

    fn parse_if_statement(&mut self) -> Result<Stmt> {
        self.advance();

        let condition = self.parse_expression()?;

        let then_branch = self.parse_block()?;

        let mut elif_branches = Vec::new();
        while self.check_keyword(&[Token::Elif]) {
            self.advance();
            let elif_cond = self.parse_expression()?;
            let elif_block = self.parse_block()?;
            elif_branches.push((elif_cond, elif_block));
        }

        let else_branch = if self.check_keyword(&[Token::Else]) {
            self.advance();
            Some(self.parse_block()?)
        } else {
            None
        };

        Ok(Stmt::If(IfStmt {
            condition,
            then_branch,
            elif_branches,
            else_branch,
        }))
    }

    fn parse_for_statement(&mut self) -> Result<Stmt> {
        self.advance();

        let var_name = self.expect_ident("Expected loop variable name")?;

        self.expect(&Token::In, "Expected 'in' after loop variable")?;

        let start = self.parse_expression()?;

        self.expect(&Token::DotDot, "Expected '..'")?;

        let end = self.parse_expression()?;

        let body = self.parse_block()?;

        Ok(Stmt::For(ForStmt {
            var_name,
            start,
            end,
            body,
        }))
    }

    fn parse_while_statement(&mut self) -> Result<Stmt> {
        self.advance();

        let condition = self.parse_expression()?;

        let body = self.parse_block()?;

        Ok(Stmt::While(WhileStmt { condition, body }))
    }

    fn parse_block(&mut self) -> Result<Block> {
        self.expect(&Token::LBrace, "Expected '{'")?;

        let mut statements = Vec::new();
        while !self.check(&Token::RBrace) && !self.is_at_end() {
            statements.push(self.parse_statement()?);
        }

        self.expect(&Token::RBrace, "Expected '}'")?;

        Ok(Block { statements })
    }

    fn parse_type(&mut self) -> Result<Type> {
        // Check for array type: [T; N]
        if self.check(&Token::LBracket) {
            self.advance(); // consume '['
            let elem_type = self.parse_type()?;
            self.expect(&Token::Semi, "Expected ';' in array type")?;
            let size = self.expect_integer("Expected array size")? as usize;
            self.expect(&Token::RBracket, "Expected ']' in array type")?;
            return Ok(Type::Array(Box::new(elem_type), size));
        }

        let ty = match self.peek() {
            Token::Ident(name) => {
                let t = Type::Named(name.clone());
                self.advance();
                t
            }
            _ => {
                return Err(HuziError::new(
                    "Expected type",
                    self.current_line(),
                    self.current_col(),
                ))
            }
        };
        Ok(ty)
    }

    fn parse_expression(&mut self) -> Result<Expr> {
        // Assignment is the lowest-precedence expression: `x = ...`, `arr[i] = ...`
        let expr = self.parse_or_expression()?;

        if self.check(&Token::Equal) {
            let target = match &expr {
                Expr::Ident(_) | Expr::ArrayIndex(_) | Expr::FieldAccess(_) => expr,
                _ => {
                    return Err(HuziError::new(
                        "Invalid assignment target",
                        self.current_line(),
                        self.current_col(),
                    ))
                }
            };
            self.advance();
            let value = self.parse_expression()?;
            return Ok(Expr::Assign(AssignExpr {
                target: Box::new(target),
                value: Box::new(value),
            }));
        }

        Ok(expr)
    }

    fn parse_or_expression(&mut self) -> Result<Expr> {
        let mut left = self.parse_and_expression()?;

        while self.check(&Token::BarBar) {
            self.advance();
            let right = self.parse_and_expression()?;
            left = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                operator: BinOp::Or,
                right: Box::new(right),
            });
        }

        Ok(left)
    }

    fn parse_and_expression(&mut self) -> Result<Expr> {
        let mut left = self.parse_equality_expression()?;

        while self.check(&Token::AmpAmp) {
            self.advance();
            let right = self.parse_equality_expression()?;
            left = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                operator: BinOp::And,
                right: Box::new(right),
            });
        }

        Ok(left)
    }

    fn parse_equality_expression(&mut self) -> Result<Expr> {
        let mut left = self.parse_comparison_expression()?;

        while self.check(&Token::EqualEqual) || self.check(&Token::BangEqual) {
            let op = if self.check(&Token::EqualEqual) {
                BinOp::Eq
            } else {
                BinOp::Neq
            };
            self.advance();
            let right = self.parse_comparison_expression()?;
            left = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                operator: op,
                right: Box::new(right),
            });
        }

        Ok(left)
    }

    fn parse_comparison_expression(&mut self) -> Result<Expr> {
        let mut left = self.parse_additive_expression()?;

        while self.check(&Token::Less)
            || self.check(&Token::LessEqual)
            || self.check(&Token::Greater)
            || self.check(&Token::GreaterEqual)
        {
            let op = if self.check(&Token::Less) {
                BinOp::Lt
            } else if self.check(&Token::LessEqual) {
                BinOp::Le
            } else if self.check(&Token::Greater) {
                BinOp::Gt
            } else {
                BinOp::Ge
            };
            self.advance();
            let right = self.parse_additive_expression()?;
            left = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                operator: op,
                right: Box::new(right),
            });
        }

        Ok(left)
    }

    fn parse_additive_expression(&mut self) -> Result<Expr> {
        let mut left = self.parse_multiplicative_expression()?;

        while self.check(&Token::Plus) || self.check(&Token::Minus) {
            let op = if self.check(&Token::Plus) {
                BinOp::Add
            } else {
                BinOp::Sub
            };
            self.advance();
            let right = self.parse_multiplicative_expression()?;
            left = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                operator: op,
                right: Box::new(right),
            });
        }

        Ok(left)
    }

    fn parse_multiplicative_expression(&mut self) -> Result<Expr> {
        let mut left = self.parse_unary_expression()?;

        while self.check(&Token::Star) || self.check(&Token::Slash) || self.check(&Token::Percent) {
            let op = if self.check(&Token::Star) {
                BinOp::Mul
            } else if self.check(&Token::Slash) {
                BinOp::Div
            } else {
                BinOp::Mod
            };
            self.advance();
            let right = self.parse_unary_expression()?;
            left = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                operator: op,
                right: Box::new(right),
            });
        }

        Ok(left)
    }

    fn parse_unary_expression(&mut self) -> Result<Expr> {
        if self.check(&Token::Bang) {
            self.advance();
            let operand = self.parse_unary_expression()?;
            Ok(Expr::Unary(UnaryExpr {
                operator: UnOp::Not,
                operand: Box::new(operand),
            }))
        } else if self.check(&Token::Minus) {
            self.advance();
            let operand = self.parse_unary_expression()?;
            Ok(Expr::Unary(UnaryExpr {
                operator: UnOp::Neg,
                operand: Box::new(operand),
            }))
        } else {
            self.parse_call_expression()
        }
    }

    fn parse_call_expression(&mut self) -> Result<Expr> {
        let mut expr = self.parse_primary_expression()?;

        // Postfix operations can chain and interleave: points[1].x, p.vals[0],
        // f(a).field, ...
        loop {
            if self.check(&Token::LBracket) {
                self.advance(); // consume '['
                let index = self.parse_expression()?;
                self.expect(&Token::RBracket, "Expected ']' after index")?;
                expr = Expr::ArrayIndex(ArrayIndexExpr {
                    array: Box::new(expr),
                    index: Box::new(index),
                });
            } else if self.check(&Token::Dot) {
                self.advance();
                let field = self.expect_ident("Expected field name after '.'")?;
                expr = Expr::FieldAccess(FieldAccessExpr {
                    base: Box::new(expr),
                    field,
                });
            } else if self.check(&Token::LParen) {
                self.advance();

                let mut arguments = Vec::new();
                while !self.check(&Token::RParen) && !self.is_at_end() {
                    arguments.push(self.parse_expression()?);

                    if self.check(&Token::Comma) {
                        self.advance();
                    }
                }

                self.expect(&Token::RParen, "Expected ')' after arguments")?;

                expr = Expr::Call(CallExpr {
                    callee: Box::new(expr),
                    arguments,
                });
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_primary_expression(&mut self) -> Result<Expr> {
        let token = self.peek().clone();

        match token {
            Token::True => {
                self.advance();
                Ok(Expr::Literal(Literal::Bool(true)))
            }
            Token::False => {
                self.advance();
                Ok(Expr::Literal(Literal::Bool(false)))
            }
            Token::Int(n) => {
                self.advance();
                Ok(Expr::Literal(Literal::Int(n)))
            }
            Token::Float(n) => {
                self.advance();
                Ok(Expr::Literal(Literal::Float(n)))
            }
            Token::String(s) => {
                self.advance();
                Ok(Expr::Literal(Literal::String(s)))
            }
            Token::Char(c) => {
                self.advance();
                Ok(Expr::Literal(Literal::Char(c)))
            }
            Token::Ident(name) => {
                self.advance();
                // `Enum::Variant` / `Enum::Variant(args)` — variant construction.
                if self.check(&Token::PathSep) {
                    self.advance();
                    let variant = self.expect_ident("Expected variant name after '::'")?;
                    let args = if self.check(&Token::LParen) {
                        self.advance();
                        let mut args = Vec::new();
                        while !self.check(&Token::RParen) && !self.is_at_end() {
                            args.push(self.parse_expression()?);
                            if self.check(&Token::Comma) {
                                self.advance();
                            }
                        }
                        self.expect(&Token::RParen, "Expected ')' after variant arguments")?;
                        args
                    } else {
                        Vec::new()
                    };
                    return Ok(Expr::EnumConstruct(EnumConstructExpr {
                        enum_name: name,
                        variant,
                        args,
                    }));
                }
                // `Point { x: 1, ... }` — a struct literal, recognized only when
                // `{` is followed by `field:`, so bare blocks still parse.
                if self.check(&Token::LBrace) && self.looks_like_struct_literal() {
                    return self.parse_struct_literal(&name);
                }
                Ok(Expr::Ident(name))
            }
            Token::LParen => {
                self.advance();
                let expr = self.parse_expression()?;
                self.expect(&Token::RParen, "Expected ')' after expression")?;
                Ok(expr)
            }
            Token::LBracket => {
                // Array literal: [1, 2, 3]
                self.advance(); // consume '['
                let mut elements = Vec::new();
                if !self.check(&Token::RBracket) {
                    loop {
                        elements.push(self.parse_expression()?);
                        if self.check(&Token::Comma) {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                }
                self.expect(&Token::RBracket, "Expected ']' after array literal")?;
                Ok(Expr::ArrayLiteral(elements))
            }
            Token::If => self.parse_if_expression(),
            Token::Match => self.parse_match_expression(),
            _ => Err(HuziError::new(
                format!("Unexpected token: {}", token),
                self.current_line(),
                self.current_col(),
            )),
        }
    }


    /// True if the upcoming tokens look like `{ field: ...` — the shape of a
    /// struct literal body (a bare block cannot start with `ident :`).
    fn looks_like_struct_literal(&self) -> bool {
        matches!(self.peek_at(1), Some(Token::Ident(_))) && matches!(self.peek_at(2), Some(Token::Colon))
    }

    /// Parse `{ field: expr, ... }` after the struct name was consumed.
    fn parse_struct_literal(&mut self, name: &str) -> Result<Expr> {
        self.expect(&Token::LBrace, "Expected '{' in struct literal")?;

        let mut fields = Vec::new();
        while !self.check(&Token::RBrace) && !self.is_at_end() {
            let field_name = self.expect_ident("Expected field name in struct literal")?;

            self.expect(&Token::Colon, "Expected ':' after field name in struct literal")?;
            let value = self.parse_expression()?;

            fields.push((field_name, value));

            if self.check(&Token::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        self.expect(&Token::RBrace, "Expected '}' after struct literal fields")?;

        Ok(Expr::StructLiteral(StructLiteralExpr {
            name: name.to_string(),
            fields,
        }))
    }

    /// `match expr { pattern => body, ... }` — each arm body is a block or a
    /// single expression.
    fn parse_match_expression(&mut self) -> Result<Expr> {
        self.advance();
        let scrutinee = self.parse_expression()?;

        self.expect(&Token::LBrace, "Expected '{' after match scrutinee")?;

        let mut arms = Vec::new();
        while !self.check(&Token::RBrace) && !self.is_at_end() {
            let pattern = self.parse_pattern()?;

            self.expect(&Token::FatArrow, "Expected '=>' after match pattern")?;

            let body = if self.check(&Token::LBrace) {
                self.parse_block()?
            } else {
                let expr = self.parse_expression()?;
                Block {
                    statements: vec![Stmt::Expr(ExprStmt { expr })],
                }
            };

            arms.push(MatchArm { pattern, body });

            if self.check(&Token::Comma) {
                self.advance();
            }
        }

        self.expect(&Token::RBrace, "Expected '}' after match arms")?;

        Ok(Expr::Match(MatchExpr {
            scrutinee: Box::new(scrutinee),
            arms,
        }))
    }

    /// `Enum::Variant`, `Enum::Variant(binding)`, or `_`.
    fn parse_pattern(&mut self) -> Result<Pattern> {
        // Note: `check` matches any Ident against Token::Ident, so the
        // wildcard must be detected by comparing the actual name.
        if let Token::Ident(name) = self.peek() {
            if name == "_" {
                self.advance();
                return Ok(Pattern::Wildcard);
            }
        }

        let enum_name = self.expect_ident("Expected pattern (variant or '_')")?;

        self.expect(&Token::PathSep, "Expected '::' after enum name in pattern")?;
        let variant = self.expect_ident("Expected variant name after '::'")?;

        let binding = if self.check(&Token::LParen) {
            self.advance();
            let binding = self.expect_ident("Expected binding name in pattern")?;
            self.expect(&Token::RParen, "Expected ')' after pattern binding")?;
            Some(binding)
        } else {
            None
        };

        Ok(Pattern::Variant {
            enum_name,
            variant,
            binding,
        })
    }

    /// If used as an expression: `let m = if c { a } else { b }`, with
    /// `elif` chains folded into a nested expression.
    fn parse_if_expression(&mut self) -> Result<Expr> {
        self.advance();
        let condition = self.parse_expression()?;
        let then_branch = self.parse_block()?;

        let mut elif_branches = Vec::new();
        while self.check(&Token::Elif) {
            self.advance();
            let elif_cond = self.parse_expression()?;
            let elif_block = self.parse_block()?;
            elif_branches.push((elif_cond, elif_block));
        }

        self.expect(&Token::Else, "Expected 'else' after if expression")?;

        let else_block = if self.check(&Token::If) {
            let nested = self.parse_if_expression()?;
            Block {
                statements: vec![Stmt::Expr(ExprStmt { expr: nested })],
            }
        } else {
            self.parse_block()?
        };

        let else_branch = Self::fold_elif_expr(&elif_branches, else_block);

        Ok(Expr::If(IfExpr {
            condition: Box::new(condition),
            then_branch,
            else_branch,
        }))
    }

    /// Fold elif branches into nested if expressions as the else block.
    fn fold_elif_expr(elifs: &[(Expr, Block)], else_b: Block) -> Block {
        match elifs.split_first() {
            None => else_b,
            Some(((cond, block), rest)) => Block {
                statements: vec![Stmt::Expr(ExprStmt {
                    expr: Expr::If(IfExpr {
                        condition: Box::new(cond.clone()),
                        then_branch: block.clone(),
                        else_branch: Self::fold_elif_expr(rest, else_b),
                    }),
                })],
            },
        }
    }

    fn is_expr_start(&self) -> bool {
        matches!(
            self.peek(),
            Token::True
                | Token::False
                | Token::Int(_)
                | Token::Float(_)
                | Token::String(_)
                | Token::Char(_)
                | Token::Ident(_)
                | Token::LParen
                | Token::LBracket
                | Token::If
                | Token::Match
                | Token::Bang
                | Token::Minus
        )
    }

    fn check(&self, token: &Token) -> bool {
        if self.is_at_end() {
            return false;
        }
        let peek_token = self.peek();
        match (peek_token, token) {
            (Token::Int(_), Token::Int(_)) => true,
            (Token::Float(_), Token::Float(_)) => true,
            (Token::String(_), Token::String(_)) => true,
            (Token::Char(_), Token::Char(_)) => true,
            (Token::Ident(_), Token::Ident(_)) => true,
            _ => std::mem::discriminant(peek_token) == std::mem::discriminant(token),
        }
    }

    fn check_keyword(&self, tokens: &[Token]) -> bool {
        tokens.iter().any(|t| self.check(t))
    }

    fn advance(&mut self) {
        if !self.is_at_end() {
            self.pos += 1;
        }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos].token
    }

    fn peek_at(&self, offset: usize) -> Option<&Token> {
        self.tokens.get(self.pos + offset).map(|t| &t.token)
    }

    fn current_line(&self) -> usize {
        self.tokens
            .get(self.pos)
            .map(|t| t.line)
            .unwrap_or(usize::MAX)
    }

    fn current_col(&self) -> usize {
        self.tokens
            .get(self.pos)
            .map(|t| t.column)
            .unwrap_or(usize::MAX)
    }

    fn expect(&mut self, token: &Token, msg: &str) -> Result<()> {
        if self.check(token) {
            self.advance();
            Ok(())
        } else {
            Err(HuziError::new(msg, self.current_line(), self.current_col()))
        }
    }

    fn expect_ident(&mut self, msg: &str) -> Result<String> {
        let token = self.tokens.get(self.pos).map(|t| t.token.clone());
        if let Some(Token::Ident(name)) = token {
            self.advance();
            Ok(name)
        } else {
            Err(HuziError::new(msg, self.current_line(), self.current_col()))
        }
    }

    fn expect_integer(&mut self, msg: &str) -> Result<i64> {
        let token = self.tokens.get(self.pos).map(|t| t.token.clone());
        if let Some(Token::Int(n)) = token {
            self.advance();
            Ok(n)
        } else {
            Err(HuziError::new(msg, self.current_line(), self.current_col()))
        }
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.tokens.len() || self.peek() == &Token::Eof
    }
}
