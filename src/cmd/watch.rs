use super::Ctx;
use crate::error::{Error, Result};
use crate::cli::WatchArgs;

pub async fn run(_ctx: &Ctx, _a: WatchArgs) -> Result<i32> {
    Err(Error::new("not_implemented", "opc watch is not built yet"))
}
