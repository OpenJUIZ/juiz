//! record の定数 (属性 `juiz.constants`、仕様 §25)。ROS 2 の定数を取り込むと付き、生成コードの定数になる
//!
//! 書き方は `@juiz.constants([{"name": "FOO", "type": "int", "value": 1}, …])`。
//! `type` は `boolean`・`int`・`long`・`float`・`double`・`string` のどれか。`doc`・`ros_type` は書いてもよい。

use juiz_types::{check_value, is_valid_type_name, DataType, RecordType};
use serde_json::Value;

/// 定数を書く属性
pub const ATTR_CONSTANTS: &str = "juiz.constants";

/// 定数 1 つ
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ConstantDef {
    pub name: String,
    pub data_type: DataType,
    pub value: Value,
    pub doc: Option<String>,
}

/// record の `juiz.constants` を読む。なければ空。書き方が違えばエラーの説明
pub(crate) fn record_constants(r: &RecordType) -> Result<Vec<ConstantDef>, String> {
    let Some(v) = r.attributes.get(ATTR_CONSTANTS) else {
        return Ok(Vec::new());
    };
    let items = v
        .as_array()
        .ok_or_else(|| format!("@{ATTR_CONSTANTS} takes a list of {{name, type, value}}"))?;
    let mut out: Vec<ConstantDef> = Vec::new();
    for item in items {
        let obj = item
            .as_object()
            .ok_or_else(|| format!("@{ATTR_CONSTANTS} takes a list of {{name, type, value}}"))?;
        let name = obj
            .get("name")
            .and_then(|n| n.as_str())
            .ok_or_else(|| format!("a constant of @{ATTR_CONSTANTS} needs a name"))?;
        if !is_valid_type_name(name) {
            return Err(format!(
                "constant name '{name}' must match [A-Za-z_][A-Za-z0-9_]*"
            ));
        }
        if out.iter().any(|c| c.name == name) || r.fields.iter().any(|f| f.name == name) {
            return Err(format!(
                "constant '{name}' is declared twice or has the name of a field"
            ));
        }
        let data_type = match obj.get("type").and_then(|t| t.as_str()) {
            Some("boolean") => DataType::Boolean,
            Some("int") => DataType::Int,
            Some("long") => DataType::Long,
            Some("float") => DataType::Float,
            Some("double") => DataType::Double,
            Some("string") => DataType::String,
            other => {
                return Err(format!(
                    "constant '{name}' has type {other:?}; use boolean, int, long, float, double or string"
                ))
            }
        };
        let value = obj
            .get("value")
            .cloned()
            .ok_or_else(|| format!("constant '{name}' needs a value"))?;
        check_value(&data_type, &value).map_err(|e| format!("constant '{name}': {e}"))?;
        out.push(ConstantDef {
            name: name.to_owned(),
            data_type,
            value,
            doc: obj
                .get("doc")
                .and_then(|d| d.as_str())
                .map(|d| d.to_owned()),
        });
    }
    Ok(out)
}
