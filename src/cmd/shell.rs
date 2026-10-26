//! `opc shell` (owned by the files sub-agent).

use super::Ctx;
use crate::error::{Error, Result};
use crate::cli::ShellArgs;

pub async fn run(_ctx: &Ctx, _a: ShellArgs) -> Result<i32> {
    Err(Error::new("not_implemented", "opc shell is not built yet"))
}

/// `/shell` in the TUI: an interactive shell in the session's workspace,
/// with the terminal handed over until it exits. Returns the exit code.
pub async fn interactive(_ctx: &Ctx, _session_id: &str) -> Result<i32> {
    Err(Error::new("not_implemented", "the shell is not built yet"))
}
