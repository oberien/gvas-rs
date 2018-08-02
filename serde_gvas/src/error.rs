use std;
use std::fmt::{self, Display};

use serde::{de, ser};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
    offset: usize,
}

#[derive(Debug)]
pub enum ErrorKind {
    Message(String),
    Io(std::io::Error),

    StringNotZeroTerminated(Vec<u8>),
    InvalidStringLength(u32),
    InvalidIntLength(u32),
    InvalidQwordLength(u32),
    InvalidFloatLength(u32),
    // Zero or more variants that can be created directly by the Serializer and
    // Deserializer without going through `ser::Error` and `de::Error`. These
    // are specific to the format, in this case JSON.
//    Eof,
//    Syntax,
//    ExpectedBoolean,
//    ExpectedInteger,
//    ExpectedString,
//    ExpectedNull,
//    ExpectedArray,
//    ExpectedArrayComma,
//    ExpectedArrayEnd,
//    ExpectedMap,
//    ExpectedMapColon,
//    ExpectedMapComma,
//    ExpectedMapEnd,
//    ExpectedEnum,
//    TrailingCharacters,
}

impl Error {
    pub fn new(kind: ErrorKind, offset: usize) -> Error {
        Error {
            kind,
            offset,
        }
    }
}

impl ser::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error::new(ErrorKind::Message(msg.to_string()), 0)
    }
}

impl de::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error::new(ErrorKind::Message(msg.to_string()), 0)
    }
}

impl Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match &self.kind {
            ErrorKind::Message(msg) => write!(fmt, "{}", msg)?,
            ErrorKind::Io(e) => e.fmt(fmt)?,
            ErrorKind::StringNotZeroTerminated(s) => write!(fmt, "string `{:?}` is not zero terminated", s)?,
            ErrorKind::InvalidStringLength(len) => write!(fmt, "invalid string length {}", len)?,
            ErrorKind::InvalidIntLength(len) => write!(fmt, "invalid int length {}", len)?,
            ErrorKind::InvalidQwordLength(len) => write!(fmt, "invalid qword length {}", len)?,
            ErrorKind::InvalidFloatLength(len) => write!(fmt, "invalid float length {}", len)?,
        }
        match self.kind {
            ErrorKind::Message(_) | ErrorKind::Io(_) => {},
            _ => write!(fmt, " at offset {}", self.offset)?,
        }
        Ok(())
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::new(ErrorKind::Io(e), 0)
    }
}
