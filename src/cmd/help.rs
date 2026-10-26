use crate::error::{Error, Result};
use crate::cli::{CompletionArgs, HelpArgs, SchemaArgs};

pub fn schema(_a: SchemaArgs) -> Result<i32> {
    Err(Error::new("not_implemented", "opc schema is not built yet"))
}

pub fn help(_a: HelpArgs) -> Result<i32> {
    Err(Error::new("not_implemented", "opc help is not built yet"))
}

pub fn completion(_a: CompletionArgs) -> Result<i32> {
    Err(Error::new("not_implemented", "opc completion is not built yet"))
}
