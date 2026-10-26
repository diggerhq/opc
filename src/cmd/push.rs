//! `opc push` and `--push` (owned by the files sub-agent).

use super::Ctx;
use crate::error::{Error, Result};
use crate::cli::PushArgs;
use serde_json::Value;
use std::path::Path;

pub async fn run(_ctx: &Ctx, _a: PushArgs) -> Result<i32> {
    Err(Error::new("not_implemented", "opc push is not built yet"))
}

/// Pushes `dir` into the session's workspace and returns the API's response
/// (used by `--push` on a new session, with `<key>:upload:<n>`).
pub async fn push_dir(_ctx: &Ctx, _session_id: &str, _dir: &Path, _to: Option<&str>, _idempotency_key: Option<String>) -> Result<Value> {
    Err(Error::new("not_implemented", "--push is not built yet"))
}
