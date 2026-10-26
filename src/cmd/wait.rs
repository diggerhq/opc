use super::Ctx;
use crate::error::{Error, Result};
use crate::cli::WaitArgs;

pub async fn run(_ctx: &Ctx, _a: WaitArgs) -> Result<i32> {
    Err(Error::new("not_implemented", "opc wait is not built yet"))
}
