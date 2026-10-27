//! core での型の扱い (データ型 Step 4、issue #193、仕様 docs/spec/datatype.md §19)
//!
//! - 型の登録所 [`TypeRegistry`] (`type/list`・`type/schema` で返す)
//! - 接続時・Topic の型検査 ([`check_connectable`]、Composition C8 #172 の定義時検査からも呼ぶ)
//! - 呼び出し時の検査 ([`check_call_arguments`]、conf の `option.type_check.on_call` で入れる)

use std::collections::BTreeMap;

use juiz_sdk::{
    anyhow::anyhow,
    manifests::ProcessManifest,
    types::{type_error, Compatibility, DataType, TypeSchema},
};
use serde::{Deserialize, Serialize};

use crate::prelude::*;

/// 型の登録所の 1 件 (`type/list` の要素)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TypeEntry {
    /// `<完全名>@sha256:<64 桁>` (仕様 §5.2)
    pub identifier: String,
    /// 完全名
    pub name: String,
    pub fingerprint: String,
    /// 表示用の短い識別子 (`Pose2D@1a2b3c4d`)
    pub short_identifier: String,
    /// `record` か `enum`
    pub kind: String,
    /// この型を使っている Factory の型名 (名前順)
    pub used_by: Vec<String>,
}

/// 型の登録所。名前付き型 (record・enum) を識別子 (完全名 + 指紋) で持つ
///
/// 同じ完全名で指紋が違う型は別の型として並ぶ。予約名の `juiz.Any`・`juiz.Image` は載せない。
/// 遅延評価の codec (L3 #177) はここに足す。
#[derive(Clone, Debug, Default)]
pub struct TypeRegistry {
    entries: BTreeMap<String, (TypeSchema, Vec<String>)>,
}

impl TypeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 型参照の中の名前付き型 (入れ子も含む) を登録する。`user` は使っている側の名前 (Factory の型名など)
    pub fn add_schema(&mut self, schema: &TypeSchema, user: &str) -> JuizResult<()> {
        let data_type = schema.validate().map_err(type_error)?;
        let mut named = Vec::new();
        collect_named_types(&data_type, &mut named);
        for t in named {
            let s = TypeSchema::from_data_type(&t).map_err(type_error)?;
            let (_, users) = self
                .entries
                .entry(s.identifier())
                .or_insert_with(|| (s, Vec::new()));
            if !user.is_empty() && !users.iter().any(|u| u == user) {
                users.push(user.to_owned());
                users.sort();
            }
        }
        Ok(())
    }

    /// manifest の引数と戻り値の型を登録する。読めない型参照はログに出して飛ばす
    pub fn add_manifest(&mut self, manifest: &ProcessManifest) {
        let schemas = manifest
            .arguments
            .iter()
            .filter_map(|a| a.schema.as_ref())
            .chain(manifest.output_schema.iter());
        for s in schemas {
            if let Err(e) = self.add_schema(s, &manifest.type_name) {
                log::warn!(
                    "TypeRegistry: type {} of {} is skipped ({e})",
                    s.name,
                    manifest.type_name
                );
            }
        }
    }

    /// 登録しているすべての型 (識別子の順)
    pub fn list(&self) -> Vec<TypeEntry> {
        self.entries
            .iter()
            .map(|(id, (s, users))| TypeEntry {
                identifier: id.clone(),
                name: s.name.clone(),
                fingerprint: s.fingerprint.clone(),
                short_identifier: s.short_identifier(),
                kind: s
                    .avro
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_owned(),
                used_by: users.clone(),
            })
            .collect()
    }

    /// 型参照を探す。`identifier` は `<完全名>@sha256:…` か完全名だけ。
    /// 完全名だけで指紋の違う型が複数あればエラー
    pub fn schema(&self, identifier: &str) -> JuizResult<TypeSchema> {
        if let Some((s, _)) = self.entries.get(identifier) {
            return Ok(s.clone());
        }
        let found: Vec<&TypeSchema> = if identifier.contains('@') {
            Vec::new()
        } else {
            self.entries
                .values()
                .map(|(s, _)| s)
                .filter(|s| s.name == identifier)
                .collect()
        };
        match found.as_slice() {
            [s] => Ok((*s).clone()),
            [] => Err(anyhow!(JuizError::TypeNotFoundError {
                identifier: identifier.to_owned(),
                message: "not registered".to_owned(),
            })),
            many => Err(anyhow!(JuizError::TypeNotFoundError {
                identifier: identifier.to_owned(),
                message: format!(
                    "ambiguous. Specify one of: {}",
                    many.iter()
                        .map(|s| s.identifier())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            })),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn collect_named_types(t: &DataType, out: &mut Vec<DataType>) {
    match t {
        DataType::Array(inner) | DataType::Map(inner) => collect_named_types(inner, out),
        DataType::Optional { inner, .. } => collect_named_types(inner, out),
        DataType::Record(r) => {
            out.push(t.clone());
            for f in r.fields.iter() {
                collect_named_types(&f.data_type, out);
            }
        }
        DataType::Enum(_) => out.push(t.clone()),
        _ => {}
    }
}

/// writer (出力側) と reader (引数側) の型参照を比べ、互換でなければ `TypeIncompatibleError` を返す (仕様 §7・§8)。
///
/// どちらかが型なしなら [`Compatibility::Untyped`] で、いつでもつながる。
/// 他の言語・ブローカーから届いた型参照も使うので、比べる前に `TypeSchema::validate` で検査する。
/// Composition C8 (#172) の定義時検査も、子の戻り値を writer、親の引数を reader にしてこれを呼ぶ。
pub fn check_connectable(
    writer: Option<&TypeSchema>,
    reader: Option<&TypeSchema>,
) -> JuizResult<Compatibility> {
    if let (Some(w), Some(r)) = (writer, reader) {
        w.validate().map_err(type_error)?;
        r.validate().map_err(type_error)?;
    }
    let c = juiz_sdk::types::compare_types(writer, reader).map_err(type_error)?;
    if let Compatibility::Incompatible { path, reason } = &c {
        return Err(anyhow!(JuizError::TypeIncompatibleError {
            writer: writer.map(|w| w.identifier()).unwrap_or_default(),
            reader: reader.map(|r| r.identifier()).unwrap_or_default(),
            path: path.clone(),
            reason: reason.clone(),
        }));
    }
    Ok(c)
}

/// Process の profile から、引数 `arg_name` の型参照を取り出す (なければ型なし)
pub fn argument_schema<'a>(profile: &'a ProcessProfile, arg_name: &str) -> Option<&'a TypeSchema> {
    profile
        .arguments
        .iter()
        .find(|a| a.name == arg_name)
        .and_then(|a| a.schema.as_ref())
}

/// 呼び出し時の検査を入れるか (conf の `option.type_check.on_call`、既定はオフ。仕様 §8)
pub fn type_check_on_call_enabled(system_manifest: &Value) -> bool {
    system_manifest
        .get("option")
        .and_then(|o| o.get("type_check"))
        .and_then(|t| t.get("on_call"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// 型付きの引数に渡す辞書の値を、引数の型と照らし合わせる (仕様 §4・§8 の呼び出し時)。
///
/// 値でないもの (画像、Rust のネイティブ値) と、型なしの引数・渡されていない引数は見ない。
pub fn check_argument_value(
    profile: &ProcessProfile,
    arg_name: &str,
    value: &CapsulePtr,
) -> JuizResult<()> {
    let Some(schema) = argument_schema(profile, arg_name) else {
        return Ok(());
    };
    if !value.is_value()? {
        return Ok(());
    }
    value.lock_as_value(|v| schema.check_value(v).map_err(type_error))?
}

/// `args` の各引数を [`check_argument_value`] で検査する
pub fn check_call_arguments(profile: &ProcessProfile, args: &CapsuleMap) -> JuizResult<()> {
    for (name, value) in args.iter() {
        check_argument_value(profile, name, value)?;
    }
    Ok(())
}
