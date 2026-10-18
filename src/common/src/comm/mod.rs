use anyhow::{Context, Result, anyhow};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::{convert::From, net::IpAddr, str::FromStr};

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
pub fn parse_address(v: &str, default: Option<(&str, u16)>) -> Result<(String, u16)> {
    if v.is_empty() {
        return default
            .map(|(h, p)| (h.to_owned(), p))
            .ok_or(anyhow!("nothing to parse and no defautl given"));
    }

    let (mut hostname, port_str) = match v.split_once(':') {
        Some((ip, port)) => (ip, port),
        None => (v, ""),
    };

    if hostname.is_empty() {
        if let Some((h, _)) = default {
            hostname = h;
        } else {
            return Err(anyhow!("no hostname set"));
        }
    }

    let port: u16 = if port_str.is_empty() {
        if let Some((_, p)) = default {
            p
        } else {
            return Err(anyhow!("not port set"));
        }
    } else {
        port_str
            .parse()
            .with_context(|| format!("could not parse port '{port_str}'"))?
    };

    Ok((hostname.to_owned(), port))
}
