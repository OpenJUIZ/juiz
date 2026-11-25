//! ネーミングサービス: 名前から System のブローカーの接続先を引く (docs/spec/naming.md)
//!
//! Step 1 (#616) は機械ごとの登録ファイル (§3〜§7)。動き続ける System は、ブローカーを立てた後に
//! `<dir>/names/<ns>/<名前>.json` を書き、止まるときに消す。引く側は pid と `system/profile` で死活を確かめる。
//!
//! Step 3 (#618) は名前で指す書き方 `name://…` (§9.1)、CLI の `-s` の名前 (§9.2)、API `naming/list`・`naming/resolve` (§9.4)。

mod dir;
mod probe;
mod record;
mod registry;
mod spec;

pub use dir::{names_dir, runtime_dir, IPC_DIR_ENV, NAMES_DIR};
pub use probe::{
    check, check_pid, core_kinds, pid_alive, CoreProbe, Liveness, Probe, ProfileProbe, LOCAL_ORDER,
    PROBE_TIMEOUT,
};
pub use record::{
    host_name, namespace_segments, read_registration, registered_name, registration_path,
    validate_name, ReadResult, RegisteredBroker, Registration, FORMAT,
};
pub use registry::{
    ambiguous_message, not_registered_message, Candidate, Conflict, ListedRegistration, NameQuery,
    NameRegistry, RegisterRequest, Registered,
};
pub use spec::{split_broker_url, NameSpec, Resolved, NAME_SCHEME};

use crate::prelude::*;

/// conf の `option.naming` (§10)。Step 1 で効くのは `register`・`unique`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamingOptions {
    pub register: bool,
    pub unique: bool,
}

impl Default for NamingOptions {
    fn default() -> Self {
        NamingOptions {
            register: true,
            unique: false,
        }
    }
}

const KNOWN_OPTION_KEYS: [&str; 4] = ["register", "unique", "advertise", "peers"];

/// conf (System の manifest) の `option.naming` を読む。知らないキーは警告をログに出して無視する
pub fn naming_options(manifest: &Value) -> NamingOptions {
    let mut o = NamingOptions::default();
    let Some(n) = manifest.get("option").and_then(|v| v.get("naming")) else {
        return o;
    };
    let Some(map) = n.as_object() else {
        log::warn!("option.naming must be a map, but {n}. Ignored.");
        return o;
    };
    for (k, v) in map {
        match k.as_str() {
            "register" => match v.as_bool() {
                Some(b) => o.register = b,
                None => {
                    log::warn!("option.naming.register must be true or false, but {v}. Ignored.")
                }
            },
            "unique" => match v.as_bool() {
                Some(b) => o.unique = b,
                None => log::warn!("option.naming.unique must be true or false, but {v}. Ignored."),
            },
            k if KNOWN_OPTION_KEYS.contains(&k) => {}
            k => log::warn!("unknown key option.naming.{k} is ignored."),
        }
    }
    o
}

/// conf の `option.naming.register` を書き換える (`--no-register`、§10)
pub fn set_register_option(manifest: &mut Value, register: bool) {
    let Some(obj) = manifest.as_object_mut() else {
        return;
    };
    let option = obj.entry("option").or_insert_with(|| jvalue!({}));
    if !option.is_object() {
        *option = jvalue!({});
    }
    let naming = option
        .as_object_mut()
        .unwrap()
        .entry("naming")
        .or_insert_with(|| jvalue!({}));
    if !naming.is_object() {
        *naming = jvalue!({});
    }
    naming
        .as_object_mut()
        .unwrap()
        .insert("register".to_owned(), jvalue!(register));
}

/// 登録名の名前の部分 (§4.1)。`juiz run` の `package.name`、無ければ conf の `name`
pub fn name_of_manifest(manifest: &Value) -> Option<String> {
    manifest
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(Value::as_str)
        .or_else(|| manifest.get("name").and_then(Value::as_str))
        .map(str::to_owned)
}

/// System の既定ネームスペース (先頭・末尾の `/` は外す)
pub fn namespace_of_manifest(manifest: &Value) -> String {
    manifest
        .get("namespace")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim_matches('/')
        .to_owned()
}

/// system/profile の `core_store.brokers` の形 (種類と名前) から、登録する `brokers` を作る (§3.3)
pub fn brokers_from_profiles<'a>(
    profiles: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Vec<RegisteredBroker> {
    profiles
        .into_iter()
        .filter(|(t, _)| !matches!(*t, "local" | "core"))
        .map(|(t, n)| RegisteredBroker::new(t, n))
        .collect()
}

/// 動き続ける System を登録する (§4・§5.1)。
///
/// 登録しないとき (`register: false`、名前が無い・規則に合わない) は `Ok(None)`。
/// 登録に失敗しても警告を出して `Ok(None)`、`unique: true` のときだけエラーを返す。
pub fn register_system(
    manifest: &Value,
    uuid: &str,
    brokers: Vec<RegisteredBroker>,
    probe: &dyn ProfileProbe,
    kinds: &[String],
) -> JuizResult<Option<Registered>> {
    let opts = naming_options(manifest);
    if !opts.register {
        return Ok(None);
    }
    let Some(name) = name_of_manifest(manifest) else {
        return Ok(None);
    };
    let namespace = namespace_of_manifest(manifest);
    if let Err(e) = validate_name(&name).and_then(|_| namespace_segments(&namespace).map(|_| ())) {
        eprintln!(
            "warning: '{}' cannot be used as a registered name ({e}); this juiz is not registered in the name list",
            registered_name(&namespace, &name)
        );
        return Ok(None);
    }
    let req = RegisterRequest {
        name: name.clone(),
        namespace: namespace.clone(),
        uuid: uuid.to_owned(),
        package: manifest.get("package").filter(|p| p.is_object()).cloned(),
        brokers,
        unique: opts.unique,
    };
    let result = NameRegistry::open().and_then(|r| r.register(&req, probe, kinds));
    match result {
        Ok(reg) => {
            let rn = reg.registration.registered_name();
            if let Some(c) = &reg.conflict {
                eprintln!(
                    "warning: {} is already registered by another juiz (pid {}); this juiz is registered as {rn}",
                    c.registered_name, c.pid
                );
            }
            log::info!("registered as {rn}");
            Ok(Some(reg))
        }
        Err(e) if opts.unique => Err(e),
        Err(e) => {
            eprintln!(
                "warning: cannot register {} in the name list: {e}",
                registered_name(&namespace, &name)
            );
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests;
