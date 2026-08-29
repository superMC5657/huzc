mod expr;
mod pattern;
mod stmt;
#[cfg(test)]
mod tests;

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

        // Tuple type: () is unit, (T1, T2, ...) is a tuple.
        if self.check(&Token::LParen) {
            self.advance(); // consume '('
            if self.check(&Token::RParen) {
                self.advance();
                return Ok(Type::Unit);
            }
            let mut elems = vec![self.parse_type()?];
            while self.check(&Token::Comma) {
                self.advance();
                elems.push(self.parse_type()?);
            }
            self.expect(&Token::RParen, "Expected ')' in tuple type")?;
            return Ok(Type::Tuple(elems));
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
