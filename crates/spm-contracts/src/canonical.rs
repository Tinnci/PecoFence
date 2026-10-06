use crate::ContractError;
use serde::Serialize;
use serde_json::Value;

pub fn canonical_json<T: Serialize>(value: &T) -> Result<String, ContractError> {
    fn sort(value: Value) -> Value {
        match value {
            Value::Object(map) => {
                Value::Object(map.into_iter().map(|(k, v)| (k, sort(v))).collect())
            }
            Value::Array(items) => Value::Array(items.into_iter().map(sort).collect()),
            other => other,
        }
    }
    let value = sort(serde_json::to_value(value)?);
    Ok(format!("{}\n", serde_json::to_string(&value)?))
}
