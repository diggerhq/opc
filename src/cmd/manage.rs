use super::Ctx;
use crate::error::{Error, Result};
use crate::cli::{DiffArgs, InterruptArgs, OpenArgs, RenameArgs, SessionOnly};

pub async fn interrupt(_ctx: &Ctx, _a: InterruptArgs) -> Result<i32> {
    Err(Error::new("not_implemented", "opc interrupt is not built yet"))
}

pub async fn rename(_ctx: &Ctx, _a: RenameArgs) -> Result<i32> {
    Err(Error::new("not_implemented", "opc rename is not built yet"))
}

pub async fn archive(_ctx: &Ctx, _a: SessionOnly, _archived: bool) -> Result<i32> {
    Err(Error::new("not_implemented", "opc archive is not built yet"))
}

pub async fn open(_ctx: &Ctx, _a: OpenArgs) -> Result<i32> {
    Err(Error::new("not_implemented", "opc open is not built yet"))
}

pub async fn diff(_ctx: &Ctx, _a: DiffArgs) -> Result<i32> {
    Err(Error::new("not_implemented", "opc diff is not built yet"))
}
