//! 型の互換性 (仕様 §7)

use anyhow::anyhow;

use super::data_type::DataType;
use super::type_schema::TypeSchema;
use crate::prelude::*;

/// writer と reader の型を比べた結果 (仕様 §7.1)
#[derive(Clone, Debug, PartialEq)]
pub enum Compatibility {
    /// 一方か両方が型なし。判定せず、今のとおりつなぐ
    Untyped,
    /// 完全名 (名前のない型は表示名) と指紋がどちらも一致する
    Same,
    /// 同じではないが、writer の値を reader として読める
    Compatible,
    /// 読めない。`path` は型の中の位置 (`Pose2D.position.x` など)
    Incompatible { path: String, reason: String },
}

impl Compatibility {
    /// 接続してよいか (型なし・同じ・互換)
    pub fn is_connectable(&self) -> bool {
        !matches!(self, Compatibility::Incompatible { .. })
    }
}

/// writer の型参照と reader の型参照を比べる (仕様 §7)。どちらかが `None` (型なし) なら [`Compatibility::Untyped`]
pub fn compare_types(
    writer: Option<&TypeSchema>,
    reader: Option<&TypeSchema>,
) -> JuizResult<Compatibility> {
    let (w, r) = match (writer, reader) {
        (Some(w), Some(r)) => (w, r),
        _ => return Ok(Compatibility::Untyped),
    };
    let wt = w.data_type()?;
    let rt = r.data_type()?;
    if w.name == r.name && w.fingerprint == r.fingerprint {
        return Ok(Compatibility::Same);
    }
    Ok(match compare_data_types(&wt, &rt) {
        Ok(()) => Compatibility::Compatible,
        Err((path, reason)) => Compatibility::Incompatible { path, reason },
    })
}

/// [`compare_types`] で互換でなければ `TypeIncompatibleError` を返す
pub fn ensure_compatible(
    writer: Option<&TypeSchema>,
    reader: Option<&TypeSchema>,
) -> JuizResult<()> {
    match compare_types(writer, reader)? {
        Compatibility::Incompatible { path, reason } => {
            Err(anyhow!(JuizError::TypeIncompatibleError {
                writer: writer.map(|w| w.identifier()).unwrap_or_default(),
                reader: reader.map(|r| r.identifier()).unwrap_or_default(),
                path,
                reason,
            }))
        }
        _ => Ok(()),
    }
}

/// 型どうしの互換性 (仕様 §7.2)。互換でなければ (位置, 理由)
pub fn compare_data_types(writer: &DataType, reader: &DataType) -> Result<(), (String, String)> {
    compare_at(writer, reader, &reader.path_root())
}

fn compare_at(w: &DataType, r: &DataType, path: &str) -> Result<(), (String, String)> {
    let incompatible = |reason: std::string::String| Err((path.to_owned(), reason));
    let names = || {
        format!(
            "{} cannot be read as {}",
            w.display_name(),
            r.display_name()
        )
    };
    use DataType::*;
    match (w, r) {
        // 規則 1・2: image と互換なのは image と any (reader) だけ
        (Image, Image) | (Image, Any) => Ok(()),
        (Image, _) | (_, Image) => incompatible(names()),
        (Any, _) | (_, Any) => Ok(()),
        // 規則 7: 省略可能
        (Null, Optional { .. }) => Ok(()),
        (Optional { inner: t, .. }, Optional { inner: u, .. }) => compare_at(t, u, path),
        (Optional { .. }, _) => incompatible(format!(
            "{} may be null but {} is not optional",
            w.display_name(),
            r.display_name()
        )),
        (_, Optional { inner: u, .. }) => compare_at(w, u, path),
        // 規則 3〜5: 基本型と数の昇格。string と bytes は互換でない
        (Null, Null)
        | (Boolean, Boolean)
        | (Int, Int)
        | (Long, Long)
        | (Float, Float)
        | (Double, Double)
        | (String, String)
        | (Bytes, Bytes) => Ok(()),
        (Int, Long)
        | (Int, Float)
        | (Int, Double)
        | (Long, Float)
        | (Long, Double)
        | (Float, Double) => Ok(()),
        // 規則 6: 配列・辞書
        (Array(t), Array(u)) => compare_at(t, u, &format!("{path}[]")),
        (Map(t), Map(u)) => compare_at(t, u, &format!("{path}{{}}")),
        // 規則 8: レコード
        (Record(wr), Record(rr)) => {
            let wname = wr.full_name();
            if rr.full_name() != wname && !rr.alias_full_names().contains(&wname) {
                return incompatible(format!(
                    "record name {wname} does not match {} (add an alias to read it)",
                    rr.full_name()
                ));
            }
            for rf in rr.fields.iter() {
                let fpath = format!("{path}.{}", rf.name);
                let wf = wr
                    .get_field(&rf.name)
                    .or_else(|| rf.aliases.iter().find_map(|a| wr.get_field(a)));
                match wf {
                    Some(wf) => compare_at(&wf.data_type, &rf.data_type, &fpath)?,
                    None if rf.default.is_some() => {}
                    None => {
                        return Err((
                            fpath,
                            format!(
                                "{wname} has no field '{}' and the reader has no default",
                                rf.name
                            ),
                        ))
                    }
                }
            }
            Ok(())
        }
        // 規則 9: 列挙
        (Enum(we), Enum(re)) => {
            let wname = we.full_name();
            if re.full_name() != wname && !re.alias_full_names().contains(&wname) {
                return incompatible(format!(
                    "enum name {wname} does not match {} (add an alias to read it)",
                    re.full_name()
                ));
            }
            if re.default.is_none() {
                if let Some(s) = we.symbols.iter().find(|s| !re.symbols.contains(s)) {
                    return incompatible(format!(
                        "symbol '{s}' of {wname} is not in {} and the reader has no default",
                        re.full_name()
                    ));
                }
            }
            Ok(())
        }
        _ => incompatible(names()),
    }
}
