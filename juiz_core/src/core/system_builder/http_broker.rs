use std::path::{Path, PathBuf};

use juiz_sdk::anyhow::Context;

use crate::{
    brokers::{
        broker_factories_wrapper::BrokerFactoriesWrapper,
        http::{http_broker_factory, http_broker_proxy_factory},
    },
    prelude::*,
};

pub fn setup_http_broker_factory(system: &mut System) -> JuizResult<()> {
    log::trace!("system_builder::setup_http_broker_factory() called");
    let hbf = http_broker_factory(system.core_broker().clone())?;
    let hbpf = http_broker_proxy_factory()?;
    let _wrapper =
        system.register_broker_factories_wrapper(BrokerFactoriesWrapper::new(None, hbf, hbpf)?)?;
    Ok(())
}

/// option.http_broker のうち、HTTP ブローカーにそのまま渡す項目
/// (static_filepaths: 静的ファイルの配信、cors_allow_origins: CORS の許可オリジン (issue #112)、
/// webui: false で埋め込みの管理画面を /ui で配信しない (issue #117)、
/// max_body_bytes: 本文の上限、既定 256 MiB (issue #233))
const HTTP_BROKER_PASS_THROUGH_OPTIONS: [&str; 4] = [
    "static_filepaths",
    "cors_allow_origins",
    "webui",
    "max_body_bytes",
];

pub(super) fn setup_http_broker(
    system: &mut System,
    port_number: i64,
    options: Option<&Value>,
) -> JuizResult<()> {
    log::trace!("system_builder::setup_http_broker() called");
    let mut manifest = jvalue!({
        "type_name": "http",
        "name": format!("0.0.0.0:{}", port_number),
        "host": "0.0.0.0",
        "port": port_number,
    });
    if let Some(http_opt) = options.and_then(|opt| obj_get_obj(opt, "http_broker").ok()) {
        let map = get_hashmap_mut(&mut manifest)?;
        for key in HTTP_BROKER_PASS_THROUGH_OPTIONS {
            if let Some(v) = http_opt.get(key) {
                map.insert(key.to_owned(), v.clone());
            }
        }
    }

    resolve_static_filepaths(&mut manifest, system.get_working_dir().as_deref());

    let _http_broker = system
        .create_broker(&manifest)
        .context("system.create_broker() failed in system_builder::setup_http_broker()")?;
    //system.register_broker(http_broker)?;
    log::info!("【作成】ブローカ(HTTP) setup_http_brokerよりHTTPBrokerを作成しました。");
    Ok(())
}

/// HTTP ブローカーのマニフェストの `static_filepaths` の相対パスを、conf の親ディレクトリ `working_dir`
/// からの相対で解決する(プラグインの path と同じ、issue #284 と mobile_sim の #4)。
///
/// - 絶対パスと、`working_dir` が無いとき(conf を使わずに System を作ったとき)はそのまま
/// - conf からの相対で見つからず、カレントディレクトリからの相対で見つかるときは、
///   0.0.29 までの挙動として警告を出してカレントディレクトリからのパスを使う
pub(super) fn resolve_static_filepaths(manifest: &mut Value, working_dir: Option<&Path>) {
    let Some(working_dir) = working_dir else {
        return;
    };
    let Some(paths) = manifest
        .get_mut("static_filepaths")
        .and_then(|v| v.as_object_mut())
    else {
        return;
    };
    for (url, value) in paths.iter_mut() {
        let Some(path) = value.as_str() else {
            continue;
        };
        let resolved = resolve_static_filepath(Path::new(path), working_dir);
        log::debug!("static_filepaths: {url} -> {resolved:?}");
        *value = Value::String(resolved.to_string_lossy().into_owned());
    }
}

fn resolve_static_filepath(path: &Path, working_dir: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_owned();
    }
    let from_conf = working_dir.join(path);
    if !from_conf.exists() && path.exists() {
        log::warn!(
            "static_filepaths の {path:?} は conf のディレクトリ {working_dir:?} からは見つからず、カレントディレクトリからは見つかったので、そちらを使います。0.0.29 より後では conf のディレクトリからの相対で書いてください"
        );
        return path.to_owned();
    }
    from_conf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_filepaths_are_relative_to_conf_dir() {
        let dir = std::env::temp_dir().join(format!("juiz_static_test_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("web")).unwrap();
        let abs = dir.join("web");
        let mut m = jvalue!({
            "type_name": "http",
            "static_filepaths": {
                "/": "web",
                "/abs": abs.to_str().unwrap(),
                "/missing": "nothing/here",
            }
        });
        resolve_static_filepaths(&mut m, Some(&dir));
        let sp = &m["static_filepaths"];
        assert_eq!(sp["/"].as_str().unwrap(), dir.join("web").to_str().unwrap());
        assert_eq!(sp["/abs"].as_str().unwrap(), abs.to_str().unwrap());
        // どちらにも無いものは conf からの相対にする
        assert_eq!(
            sp["/missing"].as_str().unwrap(),
            dir.join("nothing/here").to_str().unwrap()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn static_filepaths_fall_back_to_current_dir() {
        // カレントディレクトリ(テストではクレートのディレクトリ)にある src は、conf のディレクトリに無ければそのまま
        let dir = std::env::temp_dir().join(format!("juiz_static_fb_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut m = jvalue!({ "static_filepaths": { "/src": "src" } });
        resolve_static_filepaths(&mut m, Some(&dir));
        assert_eq!(m["static_filepaths"]["/src"].as_str().unwrap(), "src");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn static_filepaths_untouched_without_working_dir() {
        let mut m = jvalue!({ "static_filepaths": { "/": "web" } });
        resolve_static_filepaths(&mut m, None);
        assert_eq!(m["static_filepaths"]["/"].as_str().unwrap(), "web");
    }
}
