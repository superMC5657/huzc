use crate::token::{SpannedToken, Token};
use huzi_error::Result;

pub struct Lexer {
    source: Vec<char>,
    pos: usize,
    line: usize,
    column: usize,
}

impl Lexer {
    pub fn new(source: String) -> Self {
        Self {
            source: source.chars().collect(),
            pos: 0,
            line: 1,
            column: 1,
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<SpannedToken>> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token()?;
            let is_eof = token.token == Token::Eof;
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        Ok(tokens)
    }

    fn next_token(&mut self) -> Result<SpannedToken> {
        self.skip_whitespace();

        let (line, column) = (self.line, self.column);

        if self.is_at_end() {
            return Ok(SpannedToken { token: Token::Eof, line, column });
        }

        let c = self.peek();

        let token = if c.is_alphabetic() || c == '_' {
            self.read_ident()?
        } else if c.is_numeric() {
            self.read_number()?
        } else {
            match c {
                '"' => self.read_string()?,
                '\'' => self.read_char()?,
                '(' => self.single(Token::LParen),
                ')' => self.single(Token::RParen),
                '{' => self.single(Token::LBrace),
                '}' => self.single(Token::RBrace),
                '[' => self.single(Token::LBracket),
                ']' => self.single(Token::RBracket),
                ',' => self.single(Token::Comma),
                ':' => self.read_colon()?,
                ';' => self.single(Token::Semi),
                '/' => self.read_slash()?,
                '+' => self.single(Token::Plus),
                '-' => self.read_minus()?,
                '*' => self.single(Token::Star),
                '%' => self.single(Token::Percent),
                '.' => self.read_dot()?,
                '=' => self.read_equal()?,
                '!' => self.read_bang()?,
                '<' => self.read_less()?,
                '>' => self.read_greater()?,
                '&' => self.read_amp()?,
                '|' => self.read_bar()?,
                _ => {
                    return Err(huzi_error::HuziError::new(
                        format!("Unexpected character '{}'", c),
                        line,
                        column,
                    ))
                }
            }
        };

        Ok(SpannedToken { token, line, column })
    }

    fn single(&mut self, token: Token) -> Token {
        self.advance();
        token
    }

    fn skip_whitespace(&mut self) {
        while !self.is_at_end() {
            match self.peek() {
                ' ' | '\t' | '\r' => {
                    self.advance();
                }
                '\n' => {
                    self.advance();
                    self.line += 1;
                    self.column = 1;
                }
                // Support both // and # comments
                '/' => {
                    // Check for // comment
                    if self.pos + 1 < self.source.len() && self.source[self.pos + 1] == '/' {
                        self.skip_line_comment();
                    } else {
                        break;
                    }
                }
                '#' => {
                    self.skip_line_comment();
                }
                _ => break,
            }
        }
    }

    fn skip_line_comment(&mut self) {
        while !self.is_at_end() && self.peek() != '\n' {
            self.advance();
        }
        // Skip the newline too
        if !self.is_at_end() && self.peek() == '\n' {
            self.advance();
            self.line += 1;
            self.column = 1;
        }
    }

    fn read_ident(&mut self) -> Result<Token> {
        let start = self.pos;
        while !self.is_at_end() && (self.peek().is_alphanumeric() || self.peek() == '_') {
            self.advance();
        }

        let ident: String = self.source[start..self.pos].iter().collect();

        let token = match ident.as_str() {
            "fn" => Token::Fn,
            "struct" => Token::Struct,
            "enum" => Token::Enum,
            "match" => Token::Match,
            "let" => Token::Let,
            "mut" => Token::Mut,
            "if" => Token::If,
            "else" => Token::Else,
            "elif" => Token::Elif,
            "for" => Token::For,
            "in" => Token::In,
            "while" => Token::While,
            "return" => Token::Return,
            "break" => Token::Break,
            "continue" => Token::Continue,
            "true" => Token::True,
            "false" => Token::False,
            _ => Token::Ident(ident),
        };

        Ok(token)
    }

    fn read_number(&mut self) -> Result<Token> {
        let start = self.pos;
        let mut has_dot = false;

        while !self.is_at_end() {
            match self.peek() {
                '0'..='9' => {
                    self.advance();
                }
                '.' if !has_dot => {
                    // Don't consume the dot of a range expression: 1..5
                    if self.pos + 1 < self.source.len() && self.source[self.pos + 1] == '.' {
                        break;
                    }
                    has_dot = true;
                    self.advance();
                }
                _ => break,
            }
        }

        let num_str: String = self.source[start..self.pos].iter().collect();

        if has_dot {
            let val: f64 = num_str
                .parse()
                .map_err(|_| huzi_error::HuziError::new("Invalid float", self.line, self.column))?;
            Ok(Token::Float(val))
        } else {
            let val: i64 = num_str.parse().map_err(|_| {
                huzi_error::HuziError::new("Invalid integer", self.line, self.column)
            })?;
            Ok(Token::Int(val))
        }
    }

    fn read_string(&mut self) -> Result<Token> {
        self.advance();
        let mut value = String::new();

        while !self.is_at_end() && self.peek() != '"' {
            if self.peek() == '\n' {
                return Err(huzi_error::HuziError::new(
                    "Unterminated string",
                    self.line,
                    self.column,
                ));
            }
            if self.peek() == '\\' {
                self.advance();
                if self.is_at_end() {
                    return Err(huzi_error::HuziError::new(
                        "Unterminated string",
                        self.line,
                        self.column,
                    ));
                }
                let escaped = match self.peek() {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    '\\' => '\\',
                    '"' => '"',
                    '0' => '\0',
                    other => other,
                };
                value.push(escaped);
            } else {
                value.push(self.peek());
            }
            self.advance();
        }

        if self.is_at_end() {
            return Err(huzi_error::HuziError::new(
                "Unterminated string",
                self.line,
                self.column,
            ));
        }

        self.advance();

        Ok(Token::String(value))
    }

    fn read_char(&mut self) -> Result<Token> {
        self.advance();

        if self.is_at_end() {
            return Err(huzi_error::HuziError::new(
                "Unterminated char",
                self.line,
                self.column,
            ));
        }

        let c = if self.peek() == '\\' {
            self.advance();
            if self.is_at_end() {
                return Err(huzi_error::HuziError::new(
                    "Unterminated char",
                    self.line,
                    self.column,
                ));
            }
            match self.peek() {
                'n' => '\n',
                't' => '\t',
                'r' => '\r',
                '\\' => '\\',
                '\'' => '\'',
                '0' => '\0',
                other => other,
            }
        } else {
            self.peek()
        };

        self.advance();

        if self.is_at_end() || self.peek() != '\'' {
            return Err(huzi_error::HuziError::new(
                "Unterminated char",
                self.line,
                self.column,
            ));
        }

        self.advance();

        Ok(Token::Char(c))
    }

    fn read_dot(&mut self) -> Result<Token> {
        self.advance();

        if !self.is_at_end() && self.peek() == '.' {
            self.advance();
            Ok(Token::DotDot)
        } else {
            Ok(Token::Dot)
        }
    }

    fn read_colon(&mut self) -> Result<Token> {
        self.advance();

        if !self.is_at_end() && self.peek() == ':' {
            self.advance();
            Ok(Token::PathSep)
        } else {
            Ok(Token::Colon)
        }
    }

    fn read_slash(&mut self) -> Result<Token> {
        self.advance();
        // `//` comments at token start are already handled by skip_whitespace.
        Ok(Token::Slash)
    }

    fn read_minus(&mut self) -> Result<Token> {
        self.advance();

        if !self.is_at_end() && self.peek() == '>' {
            self.advance();
            Ok(Token::Arrow)
        } else {
            Ok(Token::Minus)
        }
    }

    fn read_equal(&mut self) -> Result<Token> {
        self.advance();

        if !self.is_at_end() && self.peek() == '=' {
            self.advance();
            Ok(Token::EqualEqual)
        } else if !self.is_at_end() && self.peek() == '>' {
            self.advance();
            Ok(Token::FatArrow)
        } else {
            Ok(Token::Equal)
        }
    }

    fn read_bang(&mut self) -> Result<Token> {
        self.advance();

        if !self.is_at_end() && self.peek() == '=' {
            self.advance();
            Ok(Token::BangEqual)
        } else {
            Ok(Token::Bang)
        }
    }

    fn read_less(&mut self) -> Result<Token> {
        self.advance();

        if !self.is_at_end() && self.peek() == '=' {
            self.advance();
            Ok(Token::LessEqual)
        } else {
            Ok(Token::Less)
        }
    }

    fn read_greater(&mut self) -> Result<Token> {
        self.advance();

        if !self.is_at_end() && self.peek() == '=' {
            self.advance();
            Ok(Token::GreaterEqual)
        } else {
            Ok(Token::Greater)
        }
    }

    fn read_amp(&mut self) -> Result<Token> {
        self.advance();

        if !self.is_at_end() && self.peek() == '&' {
            self.advance();
            Ok(Token::AmpAmp)
        } else {
            Err(huzi_error::HuziError::new(
                "Unexpected character '&' (did you mean '&&'?)",
                self.line,
                self.column,
            ))
        }
    }

    fn read_bar(&mut self) -> Result<Token> {
        self.advance();

        if !self.is_at_end() && self.peek() == '|' {
            self.advance();
            Ok(Token::BarBar)
        } else {
            Err(huzi_error::HuziError::new(
                "Unexpected character '|' (did you mean '||'?)",
                self.line,
                self.column,
            ))
        }
    }

    fn peek(&self) -> char {
        self.source[self.pos]
    }

    fn advance(&mut self) {
        if !self.is_at_end() {
            self.pos += 1;
            self.column += 1;
        }
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.source.len()
    }
}
