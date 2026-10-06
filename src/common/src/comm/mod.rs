use clap::{ValueEnum, error::Result};
use serde::{Deserialize, Serialize};
use std::convert::From;

pub type Port = u16;

#[derive(Debug, Copy, Clone, ValueEnum, Serialize, Deserialize)]
pub enum Side {
    A,
    B,
}

impl Side {
    #[must_use]
    pub fn port(self) -> u16 {
        match self {
            Side::A => 10010,
            Side::B => 10011,
        }
    }

    #[must_use]
    pub fn other(self) -> Side {
        match self {
            Side::A => Side::B,
            Side::B => Side::A,
        }
    }
}

impl<T: AsRef<str>> From<T> for Side {
    fn from(value: T) -> Self {
        match value.as_ref().to_lowercase().as_str() {
            "b" => Side::B,
            _ => Side::A,
        }
    }
}

impl From<Side> for &'static str {
    fn from(side: Side) -> Self {
        match side {
            Side::A => "a",
            Side::B => "b",
        }
    }
}

#[allow(clippy::unnecessary_wraps)]
pub fn parse_address(v: &str, default: (&str, u16)) -> Result<(String, u16)> {
    if let Some(ip) = v.split_once(':') {
        let p: u16 = ip.1.parse().unwrap_or(3478);
        return Ok((ip.0.to_owned(), p));
    }

    Ok((default.0.to_owned(), default.1))
}
