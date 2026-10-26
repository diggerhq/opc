//! `opc pull` (owned by the files sub-agent).

use super::Ctx;
use crate::error::{Error, Result};
use crate::cli::PullArgs;

pub async fn run(_ctx: &Ctx, _a: PullArgs) -> Result<i32> {
    Err(Error::new("not_implemented", "opc pull is not built yet"))
}
