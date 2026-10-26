use super::Ctx;
use crate::error::{Error, Result};
use crate::cli::NewArgs;

pub async fn run(_ctx: &Ctx, _a: NewArgs) -> Result<i32> {
    Err(Error::new("not_implemented", "opc new is not built yet"))
}
