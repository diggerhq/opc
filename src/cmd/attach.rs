//! `opc a` and the interactive `opc`: the TUI (owned by the TUI sub-agent).

use super::Ctx;
use crate::error::{Error, Result};
use crate::cli::{AttachArgs, RunArgs};

/// `opc a [target]`.
pub async fn run(_ctx: &mut Ctx, _a: AttachArgs) -> Result<i32> {
    Err(Error::new("not_implemented", "opc a is not built yet"))
}

/// Interactive `opc [message]`: a new session (first render before any network).
pub async fn run_new(_ctx: &mut Ctx, _run: RunArgs, _message: Option<String>) -> Result<i32> {
    Err(Error::new("not_implemented", "the interactive session is not built yet"))
}
