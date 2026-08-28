use std::fmt;

#[derive(Debug, Clone)]
pub struct HuziError {
    message: String,
    line: usize,
    column: usize,
}

impl HuziError {
    pub fn new(message: impl Into<String>, line: usize, column: usize) -> Self {
        Self {
            message: message.into(),
            line,
            column,
        }
    }

    pub fn new_global(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            line: 0,
            column: 0,
        }
    }
}

impl fmt::Display for HuziError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line > 0 {
            write!(
                f,
                "Error at line {}, column {}: {}",
                self.line, self.column, self.message
            )
        } else {
            write!(f, "Error: {}", self.message)
        }
    }
}

impl std::error::Error for HuziError {}

pub type Result<T> = std::result::Result<T, HuziError>;
