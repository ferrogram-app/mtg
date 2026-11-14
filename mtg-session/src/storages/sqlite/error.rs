// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::fmt;
use std::net::AddrParseError;

#[derive(Debug)]
pub enum SqliteSessionError {
    Poisoned,
    AddrParse(std::net::AddrParseError),
    Sql(libsql::Error),
    InvalidAuthKeyLength(usize),
}

impl std::error::Error for SqliteSessionError {}

impl fmt::Display for SqliteSessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SqliteSessionError::Poisoned => write!(f, "session lock is poisoned"),
            SqliteSessionError::AddrParse(_) => write!(f, "invalid socket address syntax"),
            SqliteSessionError::Sql(err) => write!(f, "{err}"),
            SqliteSessionError::InvalidAuthKeyLength(actual) => {
                write!(f, "invalid auth_key length: expected 256, got {actual}")
            }
        }
    }
}

impl From<AddrParseError> for SqliteSessionError {
    fn from(x: AddrParseError) -> Self {
        Self::AddrParse(x)
    }
}

impl From<libsql::Error> for SqliteSessionError {
    fn from(x: libsql::Error) -> Self {
        Self::Sql(x)
    }
}
