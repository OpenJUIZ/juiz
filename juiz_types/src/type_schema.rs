//! manifest に載せる型参照 (仕様 §6)

use serde::{Deserialize, Serialize};

use crate::data_type::{schema_invalid, DataType};
use crate::Result;
use serde_json::Value;

/// 1 つの引数・戻り値の型参照 (仕様 §6 の `schema`)
///
/// ```json
/// {"name": "robot.geometry.Pose2D", "fingerprint": "sha256:…", "avro": {"type": "record", …}}
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TypeSchema {
    /// 表示名 (仕様 §5.2)。名前付き型は完全名
    pub name: String,
    /// 指紋 `sha256:<64 桁>` (仕様 §5.1)
    pub fingerprint: String,
    /// それだけで完結した Avro のスキーマ
    pub avro: Value,
}

impl TypeSchema {
    pub fn from_data_type(data_type: &DataType) -> Result<Self> {
        Ok(Self {
            name: data_type.display_name(),
            fingerprint: data_type.fingerprint(),
            avro: data_type.to_avro()?,
        })
    }

    /// Avro のスキーマから作る。仕様 §3 に合わなければ `TypeSchemaInvalidError`
    pub fn from_avro(avro: &Value) -> Result<Self> {
        Self::from_data_type(&DataType::from_avro(avro)?)
    }

    pub fn from_avro_str(avro: &str) -> Result<Self> {
        Self::from_data_type(&DataType::from_avro_str(avro)?)
    }

    /// `avro` を読んだ型
    pub fn data_type(&self) -> Result<DataType> {
        DataType::from_avro(&self.avro)
    }

    /// `avro` が仕様 §3 に合い、`name` と `fingerprint` が `avro` から計算したものと一致するかを検査する。
    ///
    /// 他の言語やブローカーから届いた manifest の型参照は、使う前にこれで検査する。
    pub fn validate(&self) -> Result<DataType> {
        let t = self.data_type()?;
        if t.display_name() != self.name {
            return Err(schema_invalid(
                &self.name,
                format!(
                    "name '{}' does not match the schema ({})",
                    self.name,
                    t.display_name()
                ),
            ));
        }
        let fingerprint = t.fingerprint();
        if fingerprint != self.fingerprint {
            return Err(schema_invalid(
                &self.name,
                format!(
                    "fingerprint '{}' does not match the schema ({fingerprint})",
                    self.fingerprint
                ),
            ));
        }
        Ok(t)
    }

    /// 型の識別子 `<表示名>@sha256:<64 桁>` (仕様 §5.2)
    pub fn identifier(&self) -> String {
        format!("{}@{}", self.name, self.fingerprint)
    }

    /// 表示用の短い識別子 (`Pose2D@1a2b3c4d`)。同一性の判定に使ってはならない
    pub fn short_identifier(&self) -> String {
        let hex = self
            .fingerprint
            .strip_prefix("sha256:")
            .unwrap_or(&self.fingerprint);
        let short_hex: String = hex.chars().take(8).collect();
        let name = match self.data_type() {
            Ok(DataType::Record(r)) => r.name,
            Ok(DataType::Enum(e)) => e.name,
            _ => self.name.clone(),
        };
        format!("{name}@{short_hex}")
    }

    /// manifest の `type_name` (仕様 §6 の表)
    pub fn argument_type_name(&self) -> Result<&'static str> {
        Ok(self.data_type()?.argument_type_name())
    }

    /// 辞書の値がこの型に合うかを検査する (仕様 §4)
    pub fn check_value(&self, value: &Value) -> Result<()> {
        crate::check_value(&self.data_type()?, value)
    }
}
