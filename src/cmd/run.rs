use super::Ctx;
use crate::error::{Error, Result};
use crate::cli::RunArgs;

pub async fn root(_ctx: &mut Ctx, _run: RunArgs, _message: Vec<String>) -> Result<i32> {
    Err(Error::new("not_implemented", "opc is not built yet"))
}
