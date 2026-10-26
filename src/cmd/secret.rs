use super::Ctx;
use crate::error::{Error, Result};
use crate::cli::SecretCommand;

pub async fn run(_ctx: &Ctx, _a: SecretCommand) -> Result<i32> {
    Err(Error::new("not_implemented", "opc secret is not built yet"))
}
