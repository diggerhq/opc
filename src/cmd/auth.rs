use super::Ctx;
use crate::error::{Error, Result};
use crate::cli::{JsonOnly, LoginArgs};

pub async fn login(_ctx: &mut Ctx, _a: LoginArgs) -> Result<i32> {
    Err(Error::new("not_implemented", "opc login is not built yet"))
}

pub async fn logout(_ctx: &mut Ctx) -> Result<i32> {
    Err(Error::new("not_implemented", "opc logout is not built yet"))
}

pub async fn whoami(_ctx: &Ctx, _a: JsonOnly) -> Result<i32> {
    Err(Error::new("not_implemented", "opc whoami is not built yet"))
}
