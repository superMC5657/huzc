use huzi_ast::*;
use huzi_error::{HuziError, HuziResult};
use huzi_lexer::Token;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    pub fn parse(&mut self) -> HuziResult<Program> {
        let mut statements = Vec::new();

        while !self.is_at_end() {
            statements.push(self.parse_statement()?);
        }

        Ok(Program { statements })
    }

    fn parse_statement(&mut self) -> HuziResult<Stmt> {
        if self.check_keyword(&[Token::Let]) {
            self.parse_let_statement()
        } else if self.check_keyword(&[Token::Fn]) {
            self.parse_fn_statement()
        } else if self.check_keyword(&[Token::Return]) {
            self.parse_return_statement()
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

    fn parse_let_statement(&mut self) -> HuziResult<Stmt> {
        self.advance();

        let name = self.expect_ident("Expected variable name")?;
        let mutable = self.check_keyword(&[Token::Mut]);

        if mutable {
            self.advance();
        }

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

    fn parse_fn_statement(&mut self) -> HuziResult<Stmt> {
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

    fn parse_return_statement(&mut self) -> HuziResult<Stmt> {
        self.advance();

        let value = if self.is_expr_start() {
            Some(self.parse_expression()?)
        } else {
            None
        };

        Ok(Stmt::Return(ReturnStmt { value }))
    }

    fn parse_if_statement(&mut self) -> HuziResult<Stmt> {
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

    fn parse_for_statement(&mut self) -> HuziResult<Stmt> {
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

    fn parse_while_statement(&mut self) -> HuziResult<Stmt> {
        self.advance();

        let condition = self.parse_expression()?;

        let body = self.parse_block()?;

        Ok(Stmt::While(WhileStmt { condition, body }))
    }

    fn parse_block(&mut self) -> HuziResult<Block> {
        self.expect(&Token::LBrace, "Expected '{'")?;

        let mut statements = Vec::new();
        while !self.check(&Token::RBrace) && !self.is_at_end() {
            statements.push(self.parse_statement()?);
        }

        self.expect(&Token::RBrace, "Expected '}'")?;

        Ok(Block { statements })
    }

    fn parse_type(&mut self) -> HuziResult<Type> {
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

    fn parse_expression(&mut self) -> HuziResult<Expr> {
        self.parse_or_expression()
    }

    fn parse_or_expression(&mut self) -> HuziResult<Expr> {
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

    fn parse_and_expression(&mut self) -> HuziResult<Expr> {
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

    fn parse_equality_expression(&mut self) -> HuziResult<Expr> {
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

    fn parse_comparison_expression(&mut self) -> HuziResult<Expr> {
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

    fn parse_additive_expression(&mut self) -> HuziResult<Expr> {
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

    fn parse_multiplicative_expression(&mut self) -> HuziResult<Expr> {
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

    fn parse_unary_expression(&mut self) -> HuziResult<Expr> {
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

    fn parse_call_expression(&mut self) -> HuziResult<Expr> {
        let mut expr = self.parse_primary_expression()?;

        while self.check(&Token::LParen) {
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
        }

        Ok(expr)
    }

    fn parse_primary_expression(&mut self) -> HuziResult<Expr> {
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

                if self.check(&Token::Equal) && !self.is_at_end() {
                    self.advance();
                    let value = self.parse_expression()?;
                    return Ok(Expr::Assign(AssignExpr {
                        target: Box::new(Expr::Ident(name.clone())),
                        value: Box::new(value),
                    }));
                }

                Ok(Expr::Ident(name))
            }
            Token::LParen => {
                self.advance();
                let expr = self.parse_expression()?;
                self.expect(&Token::RParen, "Expected ')' after expression")?;
                Ok(expr)
            }
            Token::Print => {
                self.advance();
                self.expect(&Token::LParen, "Expected '(' after print")?;
                let expr = self.parse_expression()?;
                self.expect(&Token::RParen, "Expected ')' after print argument")?;
                Ok(Expr::Call(CallExpr {
                    callee: Box::new(Expr::Ident("print".to_string())),
                    arguments: vec![expr],
                }))
            }
            _ => Err(HuziError::new(
                format!("Unexpected token: {}", token),
                self.current_line(),
                self.current_col(),
            )),
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
                | Token::Bang
                | Token::Minus
                | Token::Print
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
        &self.tokens[self.pos]
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.tokens.len() || matches!(self.peek(), Token::Eof)
    }

    fn expect(&mut self, token: &Token, msg: &str) -> HuziResult<()> {
        if self.check(token) {
            self.advance();
            Ok(())
        } else {
            Err(HuziError::new(msg, self.current_line(), self.current_col()))
        }
    }

    fn expect_ident(&mut self, msg: &str) -> HuziResult<String> {
        let token = self.peek().clone();
        if let Token::Ident(name) = token {
            self.advance();
            Ok(name)
        } else {
            Err(HuziError::new(msg, self.current_line(), self.current_col()))
        }
    }

    fn current_line(&self) -> usize {
        1
    }

    fn current_col(&self) -> usize {
        1
    }
}
