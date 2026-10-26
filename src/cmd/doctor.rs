use super::Ctx;
use crate::error::{Error, Result};
use crate::cli::JsonOnly;

pub async fn run(_ctx: &Ctx, _a: JsonOnly) -> Result<i32> {
    Err(Error::new("not_implemented", "opc doctor is not built yet"))
}
