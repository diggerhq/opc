use super::Ctx;
use crate::error::{Error, Result};
use crate::cli::LsArgs;

pub async fn run(_ctx: &Ctx, _a: LsArgs) -> Result<i32> {
    Err(Error::new("not_implemented", "opc ls is not built yet"))
}
