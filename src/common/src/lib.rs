#![feature(random)]

use std::{thread, time::Duration};

use anyhow::Result;
use slog::{Logger, debug};

pub mod comm;
pub mod stun;

/// Retries `f` up to `attempts` times, sleeping `delay` between tries.
/// The error from the final attempt is returned.
pub fn retry<T>(
    log: &Logger,
    attempts: u32,
    delay: Duration,
    mut f: impl FnMut() -> Result<T>,
) -> Result<T> {
    for i in 1..attempts {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) => {
                debug!(log, "Attempt {i}/{attempts} failed, retrying."; "error" => %e);
                thread::sleep(delay);
            }
        }
    }

    f()
}
