use anyhow::{Result, anyhow};
use clap::ValueEnum;
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

/// Parses an address into IP and port.
#[allow(clippy::unnecessary_wraps)]
pub fn parse_address(v: &str, default: Option<(&str, u16)>) -> Result<(String, u16)> {
    if v.is_empty() {
        let Some(d) = default else {
            return Err(anyhow!("Could not parse address '{v}'."));
        };

        return Ok((d.0.to_owned(), d.1));
    }

    let ip: Vec<&str> = v.splitn(2, ':').collect();

    let mut p: u16 = 3478;
    if ip.len() > 1 {
        p = ip[1].parse().unwrap_or(p);
    }

    Ok((ip[0].to_owned(), p))
}
