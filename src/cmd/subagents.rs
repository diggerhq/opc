use super::Ctx;
use crate::error::{Error, Result};
use crate::cli::SubagentsArgs;

pub async fn run(_ctx: &Ctx, _a: SubagentsArgs) -> Result<i32> {
    Err(Error::new("not_implemented", "opc subagents is not built yet"))
}
