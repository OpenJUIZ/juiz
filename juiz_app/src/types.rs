//! `juiz type` サブコマンド: 実行中の juiz の型の登録所を見る (issue #197、仕様 docs/spec/datatype.md §23)
//!
//! ```text
//! juiz type list [--long]                                # 登録している名前付き型の識別子 (--long は表)
//! juiz type show robot.geometry.Pose2D                   # 型を JIDL の宣言で表示
//! juiz type show robot.geometry.Pose2D --avro            # .avsc (それだけで完結した Avro のスキーマ)
//! juiz type show robot.geometry.Pose2D --json-schema     # 辞書の表し方の JSON Schema
//! ```
//!
//! 型の登録所は Process・ContainerProcess の Factory の manifest から集めたもの (仕様 §19.4)。
//! 識別子は `<完全名>@sha256:…` か、同じ完全名の型が 1 つだけなら完全名だけでよい。

use std::path::Path;

use clap::Subcommand;
use juiz_core::prelude::*;
use juiz_core::types::TypeEntry;
use juiz_sdk::anyhow::anyhow;

use crate::Args;

#[derive(Debug, Subcommand, Clone)]
pub(crate) enum TypeSubCommands {
    /// List the named types (records and enums) that the Processes use
    #[clap(arg_required_else_help = false)]
    List {
        #[arg(
            long = "long",
            help = "Show kind, short identifier and the Processes that use each type"
        )]
        long: bool,
    },

    /// Show a type as JIDL declarations
    #[clap(arg_required_else_help = true)]
    Show {
        #[arg(help = "Type identifier (<full name>@sha256:..., or the full name only)")]
        identifier: String,

        #[arg(
            long = "avro",
            help = "Print the Avro schema (.avsc) instead",
            conflicts_with = "json_schema"
        )]
        avro: bool,

        #[arg(
            long = "json-schema",
            help = "Print the JSON Schema of the dictionary form instead"
        )]
        json_schema: bool,
    },
}

pub(crate) fn on_type(
    manifest: Value,
    working_dir: &Path,
    subcommand: TypeSubCommands,
    args: Args,
) -> JuizResult<()> {
    let server = args.server.clone();
    System::new(manifest)?
        .set_working_dir(working_dir)
        .start_http_broker(args.start_http_broker)
        .setup()?
        .run_and_do_once(|system| {
            let proxy = crate::composite::server_proxy(system, &server)?;
            let proxy = juiz_lock(&proxy)?;
            match subcommand {
                TypeSubCommands::List { long } => {
                    println!("{}", format_type_list(&proxy.type_list()?, long)?);
                    Ok(())
                }
                TypeSubCommands::Show {
                    identifier,
                    avro,
                    json_schema,
                } => {
                    let schema = proxy.type_schema(&identifier)?;
                    let entry = proxy
                        .type_list()?
                        .into_iter()
                        .find(|e| e.identifier == schema.identifier());
                    println!(
                        "{}",
                        format_type_show(&schema, entry.as_ref(), avro, json_schema)?
                    );
                    Ok(())
                }
            }
        })
}

/// `juiz type list` の出力。`long` でなければ識別子の JSON 配列 (他の list と同じ)
pub(crate) fn format_type_list(entries: &[TypeEntry], long: bool) -> JuizResult<String> {
    if !long {
        let ids: Vec<&str> = entries.iter().map(|e| e.identifier.as_str()).collect();
        return Ok(serde_json::to_string(&ids)?);
    }
    let rows: Vec<[String; 4]> = entries
        .iter()
        .map(|e| {
            [
                e.name.clone(),
                e.kind.clone(),
                e.short_identifier.clone(),
                e.used_by.join(","),
            ]
        })
        .collect();
    let header = ["NAME", "KIND", "SHORT_ID", "USED_BY"].map(|s| s.to_owned());
    let mut widths = header.clone().map(|h| h.chars().count());
    for r in rows.iter() {
        for (w, c) in widths.iter_mut().zip(r.iter()) {
            *w = (*w).max(c.chars().count());
        }
    }
    let line = |r: &[String; 4]| {
        let mut s = String::new();
        for (i, c) in r.iter().enumerate() {
            if i + 1 == r.len() {
                s.push_str(c);
            } else {
                s.push_str(c);
                s.push_str(&" ".repeat(widths[i] - c.chars().count() + 2));
            }
        }
        s.trim_end().to_owned()
    };
    let mut out = vec![line(&header)];
    out.extend(rows.iter().map(line));
    Ok(out.join("\n"))
}

/// `juiz type show` の出力
pub(crate) fn format_type_show(
    schema: &TypeSchema,
    entry: Option<&TypeEntry>,
    avro: bool,
    json_schema: bool,
) -> JuizResult<String> {
    if avro {
        return Ok(serde_json::to_string_pretty(&schema.avro)?);
    }
    let data_type = schema.validate().map_err(|e| anyhow!(JuizError::from(e)))?;
    if json_schema {
        return Ok(serde_json::to_string_pretty(&juiz_idl::json_schema_of(
            &data_type,
        ))?);
    }
    let mut out = format!("// {}\n", schema.identifier());
    if let Some(e) = entry {
        if !e.used_by.is_empty() {
            out.push_str(&format!("// used by: {}\n", e.used_by.join(", ")));
        }
    }
    out.push_str(&juiz_idl::to_jidl(&data_type));
    Ok(out.trim_end().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use juiz_sdk::types::{DataType, EnumType, RecordField, RecordType};

    fn pose() -> DataType {
        let mode =
            DataType::Enum(EnumType::new("a.b", "Mode", &["Idle", "Run"]).default_symbol("Idle"));
        DataType::Record(
            RecordType::new("a.b", "Pose")
                .doc("姿勢")
                .field(
                    RecordField::new("x", DataType::Double)
                        .default_value(jvalue!(0.0))
                        .unwrap(),
                )
                .field(RecordField::new("mode", mode)),
        )
    }

    fn entry(schema: &TypeSchema, used_by: &[&str]) -> TypeEntry {
        TypeEntry {
            identifier: schema.identifier(),
            name: schema.name.clone(),
            fingerprint: schema.fingerprint.clone(),
            short_identifier: schema.short_identifier(),
            kind: "record".to_owned(),
            used_by: used_by.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn list_short_and_long() {
        let s = TypeSchema::from_data_type(&pose()).unwrap();
        let entries = vec![entry(&s, &["move_to", "get_pose"])];
        assert_eq!(
            format_type_list(&entries, false).unwrap(),
            format!("[\"{}\"]", s.identifier())
        );
        let long = format_type_list(&entries, true).unwrap();
        let lines: Vec<&str> = long.lines().collect();
        assert!(lines[0].starts_with("NAME      KIND    SHORT_ID"), "{long}");
        assert!(lines[1].starts_with("a.b.Pose  record  Pose@"), "{long}");
        assert!(lines[1].ends_with("move_to,get_pose"), "{long}");
    }

    #[test]
    fn show_as_jidl_avro_and_json_schema() {
        let s = TypeSchema::from_data_type(&pose()).unwrap();
        let text = format_type_show(&s, Some(&entry(&s, &["move_to"])), false, false).unwrap();
        assert!(
            text.starts_with(&format!(
                "// {}\n// used by: move_to\nnamespace a.b;\n",
                s.identifier()
            )),
            "{text}"
        );
        assert!(
            text.contains("/** 姿勢 */\nrecord Pose {\n  double x = 0.0;\n  Mode mode;\n}"),
            "{text}"
        );
        assert!(text.contains("enum Mode { Idle, Run } = Idle;"), "{text}");
        let avro: Value =
            serde_json::from_str(&format_type_show(&s, None, true, false).unwrap()).unwrap();
        assert_eq!(avro, s.avro);
        let js: Value =
            serde_json::from_str(&format_type_show(&s, None, false, true).unwrap()).unwrap();
        assert_eq!(js["title"], "a.b.Pose");
        assert_eq!(js["$defs"]["a.b.Mode"]["enum"], jvalue!(["Idle", "Run"]));
    }
}
