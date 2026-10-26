use super::Ctx;
use crate::error::{Error, Result};
use crate::cli::AskCommand;

pub async fn run(_ctx: &Ctx, _a: AskCommand) -> Result<i32> {
    Err(Error::new("not_implemented", "opc ask is not built yet"))
}
