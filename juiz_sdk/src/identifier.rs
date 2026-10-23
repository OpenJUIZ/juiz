//! juizにおけるオブジェクトのユニークな識別子に関する定義
//!

use crate::prelude::*;
use anyhow::anyhow;
use std::sync::LazyLock;
pub type Identifier = String;

// 識別子の正規表現は呼び出しのたびにコンパイルすると 1 回で 100 µs 以上かかるので、最初の 1 回だけ作って使い回す。
/// `<broker_type>://<broker_name>/<class>/<name>::<type>` を分解する正規表現
///
/// ネームスペース付きの ID (`<class>/<namespace>/<name>`) では 4 番目に `<namespace>/<name>` が入る。
/// 分けるときは [`split_namespace`] を使う。
pub(crate) static OBJECT_IDENTIFIER_REGEX: LazyLock<Result<regex::Regex, regex::Error>> =
    LazyLock::new(|| regex::Regex::new(r"^(.+?)://(.+?)/(.+?)/(.+?)::(.+?)$"));
static BROKER_ID_REGEX: LazyLock<Result<regex::Regex, regex::Error>> =
    LazyLock::new(|| regex::Regex::new(r"^(.+?)://(.+?)$"));

/// 名前・ネームスペースの要素に使えない文字
pub const FORBIDDEN_NAME_CHARS: [char; 3] = ['/', ':', '|'];

/// インスタンス名を検査する。空、または `/` `:` `|` を含むとエラー。
pub fn validate_name(name: &str) -> JuizResult<()> {
    if name.is_empty() {
        return Err(anyhow!(JuizError::InvalidNameError {
            message: "name must not be empty".to_owned()
        }));
    }
    if let Some(c) = name.chars().find(|c| FORBIDDEN_NAME_CHARS.contains(c)) {
        return Err(anyhow!(JuizError::InvalidNameError {
            message: format!(
                "name '{name}' must not contain '{c}' ('/', ':' and '|' are not allowed)"
            )
        }));
    }
    Ok(())
}

/// ネームスペースを検査する。空文字列はルートとして許す。
///
/// `/` 区切りの各要素は空にできず、`:` `|` を含むとエラー (`robot1//arm` や先頭・末尾の `/` もエラー)。
pub fn validate_namespace(namespace: &str) -> JuizResult<()> {
    if namespace.is_empty() {
        return Ok(());
    }
    for element in namespace.split('/') {
        validate_name(element).map_err(|_| {
            anyhow!(JuizError::InvalidNameError {
                message: format!("namespace '{namespace}' has an invalid element '{element}' (elements must be non-empty and must not contain ':' or '|')")
            })
        })?;
    }
    Ok(())
}

/// manifest の `namespace` キーを読む。無ければルート (空文字列)。
///
/// 先頭の `/` (絶対指定) は外す。相対・絶対の解決は core 側で行う。
pub fn namespace_from_manifest(manifest: &Value) -> JuizResult<String> {
    let namespace = match manifest.as_object().and_then(|m| m.get("namespace")) {
        None => return Ok(String::new()),
        Some(v) => v.as_str().ok_or_else(|| {
            anyhow!(JuizError::InvalidValueError {
                message: format!("'namespace' must be a string, but {v}")
            })
        })?,
    };
    let namespace = namespace.strip_prefix('/').unwrap_or(namespace);
    validate_namespace(namespace)?;
    Ok(namespace.to_owned())
}

/// `<namespace>/<name>` を最後の `/` で `(namespace, name)` に分ける。`/` が無ければネームスペースは空。
pub fn split_namespace(path: &str) -> (String, String) {
    match path.rsplit_once('/') {
        Some((namespace, name)) => (namespace.to_owned(), name.to_owned()),
        None => (String::new(), path.to_owned()),
    }
}

/// ネームスペースと名前をつなぐ。ネームスペースが空なら名前だけ。
pub fn join_namespace(namespace: &str, name: &str) -> String {
    if namespace.is_empty() {
        name.to_owned()
    } else {
        format!("{namespace}/{name}")
    }
}

/// list の絞り込みに使う。`namespace` が `filter` と同じか、その下 (`<filter>/...`) にあれば true。
/// `filter` が空 (ルート) ならすべて true。`filter` の先頭 `/` は無視する。
pub fn is_in_namespace(namespace: &str, filter: &str) -> bool {
    let filter = filter.strip_prefix('/').unwrap_or(filter);
    filter.is_empty()
        || namespace == filter
        || namespace
            .strip_prefix(filter)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// 名前とネームスペースを検査してから、ネームスペース付きの ID を組み立てる。
///
/// 形式は `<broker_type>://<broker_name>/<class>/<namespace>/<name>::<type>`。
/// ネームスペースが空なら [`identifier_new`] と同じ文字列になる。
pub fn identifier_new_with_namespace(
    broker_type_name: &str,
    broker_name: &str,
    class_name: &str,
    type_name: &str,
    namespace: &str,
    object_name: &str,
) -> JuizResult<Identifier> {
    validate_namespace(namespace)?;
    validate_name(object_name)?;
    Ok(identifier_new(
        broker_type_name,
        broker_name,
        class_name,
        type_name,
        join_namespace(namespace, object_name).as_str(),
    ))
}

/// ID を組み立てる (名前の検査はしない)。ネームスペース付きにするときは
/// [`identifier_new_with_namespace`] を使う。
pub fn identifier_new(
    broker_type_name: &str,
    broker_name: &str,
    class_name: &str,
    type_name: &str,
    object_name: &str,
) -> Identifier {
    broker_type_name.to_string()
        + "://"
        + broker_name
        + "/"
        + class_name.to_owned().to_lowercase().as_str()
        + "/"
        + object_name
        + "::"
        + type_name
}

pub fn connection_identifier_new(
    source_id: &Identifier,
    destination_id: &Identifier,
    arg_name: &str,
) -> Identifier {
    "connection://".to_string()
        + source_id.as_str()
        + "|"
        + arg_name
        + "|"
        + destination_id.as_str()
}

/// ConnectionID -> (SourceID, DestinationID, arg_name)
pub fn connection_identifier_split(
    connection_id: Identifier,
) -> JuizResult<(String, String, String)> {
    log::trace!("connection_identifier_split({connection_id}) called");
    //todo!("not implemented connection_identifier_split");
    let tokens = connection_id[13..].split('|').collect::<Vec<&str>>();
    if tokens.len() != 3 {
        log::error!("connection_identifier_split({connection_id}) failed. The number of split tokens by '|' is not three but {}", tokens.len());
        return Err(anyhow::Error::from(
            JuizError::InvalidConnectionIdentifierError {
                identifier: connection_id,
            },
        ));
    }
    Ok((
        tokens[0].to_owned(),
        tokens[2].to_owned(),
        tokens[1].to_owned(),
    ))
    //"connnection://".to_string() + source_id.as_str() + "|" + arg_name + "|" + destination_id.as_str()
}

#[cfg(test)]
mod tests {
    use super::{connection_identifier_new, connection_identifier_split};
    use crate::prelude::*;

    #[test]
    fn connection_identifier_test() -> JuizResult<()> {
        let source_id = "core://core/ContainerProcess/read0::cv_camera_capture_read";
        let destination_id = "core://core/Process/cv_cvt_color0::cv_cvt_color";
        let arg_name = "src";
        let con_id =
            connection_identifier_new(&source_id.to_owned(), &destination_id.to_owned(), arg_name);
        let (sid, did, argn) = connection_identifier_split(con_id)?;
        assert_eq!(source_id, sid);
        assert_eq!(destination_id, did);
        assert_eq!(arg_name, argn.as_str());

        Ok(())
    }
}

pub fn identifier_from_manifest(
    default_broker_type_name: &str,
    default_broker_name: &str,
    class_name: &str,
    manifest: &Value,
) -> JuizResult<Identifier> {
    match obj_get_str(manifest, "identifier") {
        Ok(id) => Ok(id.to_string()),
        Err(_) => {
            let object_name = obj_get_str(manifest, "name")?;
            let type_name = obj_get_str(manifest, "type_name")?;

            let broker_name = obj_get_str(manifest, "broker_name")
                .or::<anyhow::Error>(Ok(default_broker_name))?;
            let broker_type_name = obj_get_str(manifest, "broker_type_name")
                .or::<anyhow::Error>(Ok(default_broker_type_name))?;
            let namespace = namespace_from_manifest(manifest)?;
            identifier_new_with_namespace(
                broker_type_name,
                broker_name,
                class_name,
                type_name,
                namespace.as_str(),
                object_name,
            )
        }
    }
}

pub(crate) fn _create_identifier(
    class_name: &str,
    type_name: &str,
    object_name: &str,
) -> Identifier {
    identifier_new("core", "core", class_name, type_name, object_name)
}

pub(crate) fn _create_factory_identifier(class_name: &str, type_name: &str) -> Identifier {
    identifier_new("core", "core", class_name, type_name, type_name)
}

#[allow(unused)]
pub(crate) fn create_identifier_from_manifest(
    class_name: &str,
    manifest: &Value,
) -> JuizResult<Identifier> {
    identifier_from_manifest("core", "core", class_name, manifest)
}

#[derive(PartialEq, Debug)]
pub struct IdentifierStruct {
    pub identifier: Identifier,
    pub class_name: String,
    pub type_name: String,
    /// ネームスペース (`/` 区切り、空ならルート)。`object_name` には含まない
    pub namespace: String,
    pub object_name: String,
    pub broker_name: String,
    pub broker_type_name: String,
}

/*
impl From<Identifier> for IdentifierStruct {
    fn from(identifier: Identifier) -> Self {
        digest_identifier(&identifier).unwrap()
    }
}
    */

impl TryFrom<Identifier> for IdentifierStruct {
    type Error = anyhow::Error;

    fn try_from(value: Identifier) -> Result<Self, Self::Error> {
        digest_identifier(&value)
    }
}

impl Into<Identifier> for IdentifierStruct {
    fn into(self) -> Identifier {
        identifier_new(
            self.broker_type_name.as_str(),
            self.broker_name.as_str(),
            self.class_name.as_str(),
            self.type_name.as_str(),
            join_namespace(&self.namespace, &self.object_name).as_str(),
        )
    }
}
impl IdentifierStruct {
    pub fn to_identifier(&self) -> Identifier {
        return identifier_new(
            self.broker_type_name.as_str(),
            self.broker_name.as_str(),
            self.class_name.as_str(),
            self.type_name.as_str(),
            join_namespace(&self.namespace, &self.object_name).as_str(),
        );
    }

    pub fn to_broker_manifest(&self) -> Value {
        return jvalue!({
            "type_name": self.broker_type_name,
            "name": self.broker_name
        });
    }

    pub fn to_manifest(&self) -> Value {
        let mut manifest = jvalue!({
            "broker_type_name": self.broker_type_name,
            "broker_name": self.broker_name,
            "type_name": self.type_name,
            "name": self.object_name
        });
        // ルートのときは今までと同じ manifest にする
        if !self.namespace.is_empty() {
            manifest["namespace"] = jvalue!(self.namespace);
        }
        manifest
    }

    /// ネームスペースを含めた名前 (`<namespace>/<name>`)
    pub fn full_name(&self) -> String {
        join_namespace(&self.namespace, &self.object_name)
    }

    /// 名前とネームスペースを検査する
    pub fn validate(&self) -> JuizResult<()> {
        validate_namespace(&self.namespace)?;
        validate_name(&self.object_name)
    }

    pub fn set_class_name<'a>(&'a mut self, class_name: &str) -> &'a IdentifierStruct {
        self.class_name = class_name.to_string();
        self
    }

    pub fn new_object(
        broker_type: &str,
        broker_name: &str,
        class_name: &str,
        object_type: &str,
        object_name: &str,
    ) -> Self {
        identifier_new(
            broker_type,
            broker_name,
            class_name,
            object_type,
            object_name,
        )
        .try_into()
        .unwrap()
    }

    /// ネームスペース付きのオブジェクトの ID。名前・ネームスペースが不正ならエラー
    pub fn new_object_with_namespace(
        broker_type: &str,
        broker_name: &str,
        class_name: &str,
        object_type: &str,
        namespace: &str,
        object_name: &str,
    ) -> JuizResult<Self> {
        identifier_new_with_namespace(
            broker_type,
            broker_name,
            class_name,
            object_type,
            namespace,
            object_name,
        )?
        .try_into()
    }

    pub fn new_broker(broker_type: &str, broker_name: &str) -> Self {
        let identifier = broker_type.to_owned() + "://" + broker_name;
        let class_name = "".to_owned();
        let type_name = "".to_owned();
        let object_name = "".to_owned();
        IdentifierStruct {
            identifier,
            class_name,
            type_name,
            namespace: String::new(),
            object_name,
            broker_name: broker_name.to_owned(),
            broker_type_name: broker_type.to_owned(),
        }
    }
    pub fn new_broker_id(id: Identifier) -> JuizResult<Self> {
        match BROKER_ID_REGEX.as_ref() {
            Ok(re) => match re.captures(&id) {
                Some(caps) => {
                    let class_name = "".to_owned();
                    let type_name = "".to_owned();
                    let object_name = "".to_owned();
                    let broker_name = caps[2].to_owned();
                    let broker_type_name = caps[1].to_owned();
                    Ok(IdentifierStruct {
                        identifier: id.clone(),
                        class_name,
                        type_name,
                        namespace: String::new(),
                        object_name,
                        broker_name,
                        broker_type_name,
                    })
                }
                None => {
                    log::error!("new_broker_id error. Invalid Identifier ({id}).");
                    return Err(anyhow!(JuizError::InvalidIdentifierError {
                        message: id.to_owned()
                    }));
                }
            },
            Err(e) => {
                log::error!("new_broker_id error. Invalid Identifier ({id}).");
                Err(anyhow!(e.clone()))
            }
        }
    }

    pub fn from_broker_identifier(identifier: &Identifier) -> JuizResult<Self> {
        log::trace!("IdentifierStruct::from_broker_identifier({identifier}) called");
        match OBJECT_IDENTIFIER_REGEX.as_ref() {
            Ok(re) => {
                match re.captures(identifier) {
                    Some(caps) => {
                        //let class_name = caps[3].to_owned();
                        let broker_type_name = caps[5].to_owned();
                        let broker_name = caps[4].to_owned();
                        // let broker_name = caps[2].to_owned();
                        //let broker_type_name = caps[1].to_owned();
                        Ok(IdentifierStruct {
                            identifier: identifier.clone(),
                            class_name: "broker".to_owned(),
                            type_name: broker_type_name.clone(),
                            namespace: String::new(),
                            object_name: broker_name.clone(),
                            broker_name,
                            broker_type_name,
                        })
                    }
                    None => {
                        log::error!("IdentifierStruct::from_broker_identifier({identifier}) failed. digest_identifier error. Invalid Identifier ({identifier}).");
                        return Err(anyhow!(JuizError::InvalidIdentifierError {
                            message: identifier.to_owned()
                        }));
                    }
                }
            }
            Err(e) => Err(anyhow!(e.clone())),
        }
    }
}

/// ID を分解する。名前・ネームスペースの文字は検査しない (受け取った ID はそのまま扱う)。
fn digest_identifier(identifier: &Identifier) -> JuizResult<IdentifierStruct> {
    match OBJECT_IDENTIFIER_REGEX.as_ref() {
        Ok(re) => match re.captures(identifier) {
            Some(caps) => {
                let class_name = caps[3].to_owned();
                let type_name = caps[5].to_owned();
                let (namespace, object_name) = split_namespace(&caps[4]);
                let broker_name = caps[2].to_owned();
                let broker_type_name = caps[1].to_owned();
                Ok(IdentifierStruct {
                    identifier: identifier.clone(),
                    class_name,
                    type_name,
                    namespace,
                    object_name,
                    broker_name,
                    broker_type_name,
                })
            }
            None => {
                log::error!("digest_identifier error. Invalid Identifier ({identifier}).");
                return Err(anyhow!(JuizError::InvalidIdentifierError {
                    message: identifier.to_owned()
                }));
            }
        },
        Err(e) => Err(anyhow!(e.clone())),
    }
}
