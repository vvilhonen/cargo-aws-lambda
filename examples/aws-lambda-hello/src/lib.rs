use lambda_runtime::error::HandlerError;
use lambda_runtime::Context;
use serde_json::{json, Value};

pub fn handler(value: Value, _context: Context) -> Result<Value, HandlerError> {
    log::info!("Logging from lambda");
    Ok(json!({
        "ok": true,
        "input": value
    }))
}
