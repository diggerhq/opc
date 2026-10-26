//! `opc shell` (owned by the files sub-agent).

use super::Ctx;
use crate::error::{Error, Result};
use crate::cli::ShellArgs;

pub async fn run(_ctx: &Ctx, _a: ShellArgs) -> Result<i32> {
    Err(Error::new("not_implemented", "opc shell is not built yet"))
}
