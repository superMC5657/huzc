use crate::token::Token;
use huzi_error::{HuziError, HuziResult};

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

    pub fn tokenize(&mut self) -> HuziResult<Vec<Token>> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token()?;
            if token == Token::Eof {
                tokens.push(token);
                break;
            }
            tokens.push(token);
        }
        Ok(tokens)
    }

    fn next_token(&mut self) -> HuziResult<Token> {
        self.skip_whitespace();

        if self.is_at_end() {
            return Ok(Token::Eof);
        }

        let c = self.peek();

        if c.is_alphabetic() || c == '_' {
            return self.read_ident();
        }

        if c.is_numeric() {
            return self.read_number();
        }

        match c {
            '"' => self.read_string(),
            '\'' => self.read_char(),
            '(' => {
                self.advance();
                Ok(Token::LParen)
            }
            ')' => {
                self.advance();
                Ok(Token::RParen)
            }
            '{' => {
                self.advance();
                Ok(Token::LBrace)
            }
            '}' => {
                self.advance();
                Ok(Token::RBrace)
            }
            '[' => {
                self.advance();
                Ok(Token::LBracket)
            }
            ']' => {
                self.advance();
                Ok(Token::RBracket)
            }
            ',' => {
                self.advance();
                Ok(Token::Comma)
            }
            ':' => {
                self.advance();
                Ok(Token::Colon)
            }
            '.' => self.read_dot(),
            '+' => {
                self.advance();
                Ok(Token::Plus)
            }
            '-' => self.read_minus(),
            '*' => {
                self.advance();
                Ok(Token::Star)
            }
            '/' => {
                self.advance();
                Ok(Token::Slash)
            }
            '%' => {
                self.advance();
                Ok(Token::Percent)
            }
            '=' => self.read_equal(),
            '!' => self.read_bang(),
            '<' => self.read_less(),
            '>' => self.read_greater(),
            '&' => self.read_amp(),
            '|' => self.read_bar(),
            '\n' => {
                self.advance();
                self.line += 1;
                self.column = 1;
                self.next_token()
            }
            _ => {
                self.advance();
                Ok(Token::Unknown)
            }
        }
    }

    fn skip_whitespace(&mut self) {
        while !self.is_at_end() {
            match self.peek() {
                ' ' | '\t' | '\r' => {
                    self.advance();
                    self.column += 1;
                }
                _ => break,
            }
        }
    }

    fn read_ident(&mut self) -> HuziResult<Token> {
        let start = self.pos;
        let start_col = self.column;
        while !self.is_at_end() && (self.peek().is_alphanumeric() || self.peek() == '_') {
            self.advance();
            self.column += 1;
        }

        let ident: String = self.source[start..self.pos].iter().collect();

        let token = match ident.as_str() {
            "fn" => Token::Fn,
            "let" => Token::Let,
            "mut" => Token::Mut,
            "if" => Token::If,
            "else" => Token::Else,
            "elif" => Token::Elif,
            "for" => Token::For,
            "in" => Token::In,
            "while" => Token::While,
            "return" => Token::Return,
            "true" => Token::True,
            "false" => Token::False,
            "print" => Token::Print,
            _ => Token::Ident(ident),
        };

        Ok(token)
    }

    fn read_number(&mut self) -> HuziResult<Token> {
        let start = self.pos;
        let mut has_dot = false;

        while !self.is_at_end() {
            match self.peek() {
                '0'..='9' => {
                    self.advance();
                    self.column += 1;
                }
                '.' if !has_dot => {
                    has_dot = true;
                    self.advance();
                    self.column += 1;
                }
                _ => break,
            }
        }

        let num_str: String = self.source[start..self.pos].iter().collect();

        if has_dot {
            let val: f64 = num_str
                .parse()
                .map_err(|_| HuziError::new("Invalid float", self.line, self.column))?;
            Ok(Token::Float(val))
        } else {
            let val: i64 = num_str
                .parse()
                .map_err(|_| HuziError::new("Invalid integer", self.line, self.column))?;
            Ok(Token::Int(val))
        }
    }

    fn read_string(&mut self) -> HuziResult<Token> {
        self.advance();
        self.column += 1;
        let mut value = String::new();

        while !self.is_at_end() && self.peek() != '"' {
            if self.peek() == '\n' {
                return Err(HuziError::new(
                    "Unterminated string",
                    self.line,
                    self.column,
                ));
            }
            if self.peek() == '\\' {
                self.advance();
                self.column += 1;
                if self.is_at_end() {
                    return Err(HuziError::new(
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
                    _ => self.peek(),
                };
                value.push(escaped);
            } else {
                value.push(self.peek());
            }
            self.advance();
            self.column += 1;
        }

        if self.is_at_end() {
            return Err(HuziError::new(
                "Unterminated string",
                self.line,
                self.column,
            ));
        }

        self.advance();
        self.column += 1;

        Ok(Token::String(value))
    }

    fn read_char(&mut self) -> HuziResult<Token> {
        self.advance();
        self.column += 1;

        if self.is_at_end() {
            return Err(HuziError::new("Unterminated char", self.line, self.column));
        }

        let c = if self.peek() == '\\' {
            self.advance();
            self.column += 1;
            if self.is_at_end() {
                return Err(HuziError::new("Unterminated char", self.line, self.column));
            }
            match self.peek() {
                'n' => '\n',
                't' => '\t',
                'r' => '\r',
                '\\' => '\\',
                '\'' => '\'',
                _ => self.peek(),
            }
        } else {
            self.peek()
        };

        self.advance();
        self.column += 1;

        if self.is_at_end() || self.peek() != '\'' {
            return Err(HuziError::new("Unterminated char", self.line, self.column));
        }

        self.advance();
        self.column += 1;

        Ok(Token::Char(c))
    }

    fn read_dot(&mut self) -> HuziResult<Token> {
        self.advance();
        self.column += 1;

        if !self.is_at_end() && self.peek() == '.' {
            self.advance();
            self.column += 1;
            Ok(Token::DotDot)
        } else {
            Ok(Token::Dot)
        }
    }

    fn read_minus(&mut self) -> HuziResult<Token> {
        self.advance();
        self.column += 1;

        if !self.is_at_end() && self.peek() == '>' {
            self.advance();
            self.column += 1;
            Ok(Token::Arrow)
        } else {
            Ok(Token::Minus)
        }
    }

    fn read_equal(&mut self) -> HuziResult<Token> {
        self.advance();
        self.column += 1;

        if !self.is_at_end() && self.peek() == '=' {
            self.advance();
            self.column += 1;
            Ok(Token::EqualEqual)
        } else {
            Ok(Token::Equal)
        }
    }

    fn read_bang(&mut self) -> HuziResult<Token> {
        self.advance();
        self.column += 1;

        if !self.is_at_end() && self.peek() == '=' {
            self.advance();
            self.column += 1;
            Ok(Token::BangEqual)
        } else {
            Ok(Token::Bang)
        }
    }

    fn read_less(&mut self) -> HuziResult<Token> {
        self.advance();
        self.column += 1;

        if !self.is_at_end() && self.peek() == '=' {
            self.advance();
            self.column += 1;
            Ok(Token::LessEqual)
        } else {
            Ok(Token::Less)
        }
    }

    fn read_greater(&mut self) -> HuziResult<Token> {
        self.advance();
        self.column += 1;

        if !self.is_at_end() && self.peek() == '=' {
            self.advance();
            self.column += 1;
            Ok(Token::GreaterEqual)
        } else {
            Ok(Token::Greater)
        }
    }

    fn read_amp(&mut self) -> HuziResult<Token> {
        self.advance();
        self.column += 1;

        if !self.is_at_end() && self.peek() == '&' {
            self.advance();
            self.column += 1;
            Ok(Token::AmpAmp)
        } else {
            Ok(Token::Unknown)
        }
    }

    fn read_bar(&mut self) -> HuziResult<Token> {
        self.advance();
        self.column += 1;

        if !self.is_at_end() && self.peek() == '|' {
            self.advance();
            self.column += 1;
            Ok(Token::BarBar)
        } else {
            Ok(Token::Unknown)
        }
    }

    fn peek(&self) -> char {
        self.source[self.pos]
    }

    fn advance(&mut self) {
        if !self.is_at_end() {
            self.pos += 1;
        }
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.source.len()
    }
}
