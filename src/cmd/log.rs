use super::Ctx;
use crate::error::{Error, Result};
use crate::cli::LogArgs;

pub async fn run(_ctx: &Ctx, _a: LogArgs) -> Result<i32> {
    Err(Error::new("not_implemented", "opc log is not built yet"))
}
