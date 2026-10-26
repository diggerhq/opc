use super::Ctx;
use crate::error::{Error, Result};
use crate::cli::SendArgs;

pub async fn run(_ctx: &Ctx, _a: SendArgs) -> Result<i32> {
    Err(Error::new("not_implemented", "opc send is not built yet"))
}
