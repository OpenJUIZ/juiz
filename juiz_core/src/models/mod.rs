//! Container の 3D モデル (ContainerModel、docs/spec/container-model.md、issue #381)
//!
//! - core はモデルの中身を解釈しない (§2 の 1)。読むのは形式の判定に要る先頭 (と SDF の最上位の要素)・
//!   大きさ・SHA-256 だけ。
//! - 作成時 (conf の読み込み) に確かめ ([`ContainerModel::load`])、覚えるのは解決したパス・形式・大きさ・
//!   更新時刻・SHA-256 と conf の値だけ。中身は持たない (§5.2)。
//! - 取り出しのたびに大きさと更新時刻を見て、変わっていれば確かめ直す (§5.3)。
//! - 登録所 [`ContainerModels`] は CoreStore が持ち、Container の ID (core の ID の文字列) で引く。

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use base64::Engine;
use juiz_sdk::anyhow::anyhow;
use juiz_sdk::geometry::Transform;
use juiz_sdk::manifests::{ModelFormat, ModelManifest};
use juiz_sdk::types::{JuizType, TypeSchema};
use sha2::{Digest, Sha256};

use crate::prelude::*;

/// モデルのファイルの大きさの既定の上限 (64 MiB、§5.4)
pub const DEFAULT_MODEL_MAX_BYTES: u64 = 64 * 1024 * 1024;

/// JSON の経路 (base64) でこれより大きいモデルを返すときは WARN を出す (16 MiB、§5.4)
pub const JSON_WARN_BYTES: u64 = 16 * 1024 * 1024;

/// 型の登録所に載せるときの「使っている側」の名前
pub const CONTAINER_MODEL_TYPE_USER: &str = "container_model";

/// API で使う `juiz_std.model` の型 (`ContainerModelInfo`、§8)
pub fn container_model_type_schemas() -> JuizResult<Vec<TypeSchema>> {
    Ok(vec![juiz_std_types::model::ContainerModelInfo::type_schema()?])
}

/// conf の `option.model.max_bytes` (無ければ既定、§5.4)。正の整数でなければエラー
pub fn max_bytes_from_options(options: Option<&Value>) -> JuizResult<u64> {
    let Some(v) = options.and_then(|o| o.get("model")).and_then(|m| m.get("max_bytes")) else {
        return Ok(DEFAULT_MODEL_MAX_BYTES);
    };
    match v.as_u64() {
        Some(n) if n > 0 => Ok(n),
        _ => Err(invalid(format!(
            "option.model.max_bytes must be a positive integer, but {v}"
        ))),
    }
}

/// API (ブローカー越しの `container/create` など) の manifest に `model` があればエラー (§5.5)。
/// 遠くの利用者が任意のパスを指して、juiz を動かす機械の上のファイルを読み出せないようにする
pub fn reject_model_from_api(manifest: &Value) -> JuizResult<()> {
    if manifest.get("model").is_some_and(|m| !m.is_null()) {
        return Err(invalid(
            "'model' can only be written in the conf file, not given through the API \
             (docs/spec/container-model.md §5.5)"
                .to_owned(),
        ));
    }
    Ok(())
}

fn invalid(message: String) -> juiz_sdk::anyhow::Error {
    anyhow!(JuizError::ContainerModelInvalidError { message })
}

fn not_found(message: String) -> juiz_sdk::anyhow::Error {
    anyhow!(JuizError::ContainerModelFileNotFoundError { message })
}

/// SHA-256 の小文字 16 進 64 文字
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xf) as usize] as char);
    }
    s
}

/// ファイル全体の SHA-256 (読みながら計算する)
fn sha256_hex_file(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

/// ファイルの先頭 `n` バイト (ファイルが短ければ全部)
fn read_head(path: &Path, n: usize) -> std::io::Result<Vec<u8>> {
    let f = std::fs::File::open(path)?;
    let mut buf = Vec::with_capacity(n);
    f.take(n as u64).read_to_end(&mut buf)?;
    Ok(buf)
}

/// 先頭の確かめ (§3.1)。合わなければ `ContainerModelInvalidError`。
/// `warn` は作成時にだけ出す警告 (外部参照の .gltf、§3.3) を出すか
fn check_format(
    path: &Path,
    file_name: &str,
    format: ModelFormat,
    size: u64,
    warn: bool,
) -> JuizResult<()> {
    let io = |e: std::io::Error| not_found(format!("can not read model file '{file_name}': {e}"));
    let mismatch = |why: String| invalid(format!("model file '{file_name}' is not {format}: {why}"));
    match format {
        ModelFormat::Glb => {
            let head = read_head(path, 8).map_err(io)?;
            if head.len() < 8 || &head[0..4] != b"glTF" {
                return Err(mismatch("the first 4 bytes are not 'glTF'".to_owned()));
            }
            let version = u32::from_le_bytes([head[4], head[5], head[6], head[7]]);
            if version != 2 {
                return Err(mismatch(format!("glTF version is {version}, not 2")));
            }
        }
        ModelFormat::Stl => {
            let head = read_head(path, 84).map_err(io)?;
            let binary = head.len() == 84 && {
                let n = u32::from_le_bytes([head[80], head[81], head[82], head[83]]) as u64;
                size == 84 + 50 * n
            };
            if !binary && !head.starts_with(b"solid") {
                return Err(mismatch(
                    "neither a binary STL (size = 84 + 50 * n) nor an ASCII STL (starts with 'solid')"
                        .to_owned(),
                ));
            }
        }
        ModelFormat::Gltf => {
            let bytes = std::fs::read(path).map_err(io)?;
            let v: Value = serde_json::from_slice(&bytes)
                .map_err(|e| mismatch(format!("not a JSON ({e})")))?;
            let version = v
                .pointer("/asset/version")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if !v.is_object() || !version.starts_with('2') {
                return Err(mismatch(format!(
                    "asset.version must be a string starting with '2', but '{version}'"
                )));
            }
            if warn {
                let external = ["buffers", "images"]
                    .iter()
                    .filter_map(|k| v.get(*k).and_then(|a| a.as_array()))
                    .flatten()
                    .filter_map(|e| e.get("uri").and_then(|u| u.as_str()))
                    .any(|u| !u.starts_with("data:"));
                if external {
                    log::warn!(
                        "model file '{file_name}' refers to external files (buffers[].uri / images[].uri). \
                         Only this file is served; the Web UI does not fetch the external files \
                         (docs/spec/container-model.md §3.3)"
                    );
                }
            }
        }
        ModelFormat::Sdf => {
            let bytes = std::fs::read(path).map_err(io)?;
            let text = std::str::from_utf8(&bytes)
                .map_err(|e| mismatch(format!("not a UTF-8 text ({e})")))?;
            let doc = roxmltree::Document::parse(text)
                .map_err(|e| mismatch(format!("not an XML ({e})")))?;
            let root = doc.root_element();
            if root.tag_name().name() != "sdf" {
                return Err(mismatch(format!(
                    "the top element must be <sdf>, but <{}>",
                    root.tag_name().name()
                )));
            }
            let models = root
                .children()
                .filter(|n| n.is_element() && n.tag_name().name() == "model")
                .count();
            if models != 1 {
                return Err(mismatch(format!(
                    "<sdf> must have exactly one <model> directly under it, but {models} \
                     (<world> or several models are not supported, docs/spec/container-model.md §3.2)"
                )));
            }
        }
        ModelFormat::Other => {}
    }
    Ok(())
}

/// 覚えておくファイルの状態 (§5.2 の 5)
#[derive(Debug, Clone, PartialEq)]
struct FileState {
    size: u64,
    modified: Option<SystemTime>,
    sha256: String,
}

/// Container 1 つのモデル。中身は持たず、取り出すたびにファイルから読む
#[derive(Debug, Clone)]
pub struct ContainerModel {
    /// 解決した絶対パス (外には出さない)
    path: PathBuf,
    file_name: String,
    format: ModelFormat,
    up_axis: String,
    scale: f64,
    origin: Transform,
    color: Option<String>,
    state: FileState,
}

impl ContainerModel {
    /// conf の `model:` から作る (§5.2)。相対パスは `base_dir` (conf の親ディレクトリ) からの相対。
    /// `base_dir` が無ければ (conf を使わずに System を作ったとき) カレントディレクトリからの相対
    pub fn load(
        manifest: &ModelManifest,
        base_dir: Option<&Path>,
        max_bytes: u64,
    ) -> JuizResult<ContainerModel> {
        let given = Path::new(&manifest.path);
        let joined = match base_dir {
            Some(dir) if given.is_relative() => dir.join(given),
            _ => given.to_path_buf(),
        };
        let path = std::path::absolute(&joined).unwrap_or(joined);
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let format = manifest
            .format
            .unwrap_or_else(|| ModelFormat::from_path(&path));
        if manifest.format.is_none()
            && path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("urdf"))
        {
            log::warn!(
                "model '{}' is a URDF. URDF describes several Containers and Transforms; \
                 use the import (Step 9 #388) instead. It is served as 'other'",
                manifest.path
            );
        }
        let mut model = ContainerModel {
            file_name,
            format,
            up_axis: manifest
                .up_axis
                .clone()
                .unwrap_or_else(|| format.default_up_axis().to_owned()),
            scale: manifest.scale.unwrap_or(1.0),
            origin: manifest.origin.unwrap_or_default(),
            color: manifest.color.clone(),
            state: FileState {
                size: 0,
                modified: None,
                sha256: String::new(),
            },
            path,
        };
        model.state = model
            .check(max_bytes, true)
            .with_context_path(&manifest.path)?;
        Ok(model)
    }

    /// §5.2 の 2〜5 (ファイルがあり、上限以下で、形式に合う。SHA-256 を計算する)
    fn check(&self, max_bytes: u64, creating: bool) -> JuizResult<FileState> {
        let meta = std::fs::metadata(&self.path).map_err(|e| {
            not_found(format!("model file '{}' can not be opened: {e}", self.file_name))
        })?;
        if !meta.is_file() {
            return Err(not_found(format!(
                "model file '{}' is not a regular file",
                self.file_name
            )));
        }
        let size = meta.len();
        if size > max_bytes {
            return Err(invalid(format!(
                "model file '{}' is too large ({size} bytes > {max_bytes} bytes). \
                 Raise option.model.max_bytes",
                self.file_name
            )));
        }
        check_format(&self.path, &self.file_name, self.format, size, creating)?;
        let sha256 = sha256_hex_file(&self.path).map_err(|e| {
            not_found(format!("can not read model file '{}': {e}", self.file_name))
        })?;
        Ok(FileState {
            size,
            modified: meta.modified().ok(),
            sha256,
        })
    }

    /// 大きさか更新時刻が変わっていれば確かめ直す (§5.3)。`force` なら変わっていなくても
    fn refresh(&mut self, max_bytes: u64, force: bool) -> JuizResult<()> {
        let meta = std::fs::metadata(&self.path).map_err(|e| {
            not_found(format!("model file '{}' can not be opened: {e}", self.file_name))
        })?;
        if !force && meta.len() == self.state.size && meta.modified().ok() == self.state.modified {
            return Ok(());
        }
        self.state = self.check(max_bytes, false)?;
        Ok(())
    }

    pub fn format(&self) -> ModelFormat {
        self.format
    }

    pub fn size(&self) -> u64 {
        self.state.size
    }

    pub fn sha256(&self) -> &str {
        &self.state.sha256
    }

    /// モデル情報 (`juiz_std.model.ContainerModelInfo` の形、§8)
    pub fn info(&self) -> Value {
        jvalue!({
            "format": self.format.name(),
            "media_type": self.format.media_type(),
            "size": self.state.size,
            "sha256": self.state.sha256,
            "file_name": self.file_name,
            "scale": self.scale,
            "origin": serde_json::to_value(self.origin).unwrap_or(Value::Null),
            "up_axis": self.up_axis,
            "color": self.color.clone().unwrap_or_default(),
        })
    }
}

trait WithContextPath<T> {
    fn with_context_path(self, path: &str) -> JuizResult<T>;
}

impl<T> WithContextPath<T> for JuizResult<T> {
    fn with_context_path(self, path: &str) -> JuizResult<T> {
        use juiz_sdk::anyhow::Context;
        self.with_context(|| format!("model.path = '{path}'"))
    }
}

/// 取り出したモデル (中身とモデル情報)
#[derive(Debug, Clone)]
pub struct ContainerModelData {
    /// モデル情報 (§8)
    pub info: Value,
    /// ファイルの中身
    pub data: Vec<u8>,
}

impl ContainerModelData {
    /// JSON の経路の形 `{"info": …, "data": "<base64>"}` (§6.3)
    pub fn to_value(&self) -> Value {
        jvalue!({
            "info": self.info,
            "data": base64::engine::general_purpose::STANDARD.encode(&self.data),
        })
    }

    /// JSON の経路の形から (§6.3)。`data` の base64 が読めなければエラー
    pub fn from_value(v: &Value) -> JuizResult<ContainerModelData> {
        let info = v.get("info").cloned().unwrap_or(Value::Null);
        let data = v.get("data").and_then(|d| d.as_str()).ok_or_else(|| {
            invalid(format!(
                "container/model response must be {{info, data: <base64>}}, but has no 'data'"
            ))
        })?;
        let data = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| invalid(format!("container/model response has invalid base64 ({e})")))?;
        Ok(ContainerModelData { info, data })
    }

    /// `info.sha256` (無ければ空)
    pub fn sha256(&self) -> &str {
        self.info
            .get("sha256")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
    }

    /// `info.media_type` (無ければ application/octet-stream)
    pub fn media_type(&self) -> &str {
        self.info
            .get("media_type")
            .and_then(|s| s.as_str())
            .unwrap_or("application/octet-stream")
    }

    /// `info.file_name` (無ければ空)
    pub fn file_name(&self) -> &str {
        self.info
            .get("file_name")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
    }
}

#[derive(Debug)]
struct Inner {
    models: HashMap<String, ContainerModel>,
    max_bytes: u64,
    /// 16 MiB 超を JSON の経路で返したと WARN を出した Container (1 Container につき 1 回、§5.4)
    json_warned: HashSet<String>,
}

/// Container のモデルの登録所。Container の ID (core の ID の文字列) で引く
#[derive(Debug)]
pub struct ContainerModels {
    inner: Mutex<Inner>,
}

pub type ContainerModelsPtr = Arc<ContainerModels>;

impl ContainerModels {
    pub fn new() -> ContainerModelsPtr {
        Arc::new(ContainerModels {
            inner: Mutex::new(Inner {
                models: HashMap::new(),
                max_bytes: DEFAULT_MODEL_MAX_BYTES,
                json_warned: HashSet::new(),
            }),
        })
    }

    fn lock(&self) -> JuizResult<std::sync::MutexGuard<'_, Inner>> {
        self.inner
            .lock()
            .map_err(|e| anyhow!(JuizError::ObjectLockError { target: e.to_string() }))
    }

    /// 上限 (`option.model.max_bytes`、§5.4)
    pub fn max_bytes(&self) -> JuizResult<u64> {
        Ok(self.lock()?.max_bytes)
    }

    pub fn set_max_bytes(&self, max_bytes: u64) -> JuizResult<()> {
        self.lock()?.max_bytes = max_bytes;
        Ok(())
    }

    pub fn insert(&self, container_id: &str, model: ContainerModel) -> JuizResult<()> {
        let mut inner = self.lock()?;
        inner.json_warned.remove(container_id);
        inner.models.insert(container_id.to_owned(), model);
        Ok(())
    }

    pub fn remove(&self, container_id: &str) -> JuizResult<Option<ContainerModel>> {
        let mut inner = self.lock()?;
        inner.json_warned.remove(container_id);
        Ok(inner.models.remove(container_id))
    }

    pub fn contains(&self, container_id: &str) -> JuizResult<bool> {
        Ok(self.lock()?.models.contains_key(container_id))
    }

    pub fn clear(&self) -> JuizResult<()> {
        let mut inner = self.lock()?;
        inner.models.clear();
        inner.json_warned.clear();
        Ok(())
    }

    /// 取り出しの失敗は WARN に出す (Container は消さない、§5.3)
    fn warn_on_error<T>(container_id: &str, r: JuizResult<T>) -> JuizResult<T> {
        if let Err(e) = &r {
            log::warn!("ContainerModel of Container({container_id}) can not be served: {e:#}");
        }
        r
    }

    /// モデル情報 (§8)。モデルが無ければ `None`。ファイルが変わっていれば確かめ直す
    pub fn info(&self, container_id: &str) -> JuizResult<Option<Value>> {
        let mut inner = self.lock()?;
        let max = inner.max_bytes;
        let Some(m) = inner.models.get_mut(container_id) else {
            return Ok(None);
        };
        Self::warn_on_error(container_id, m.refresh(max, false))?;
        Ok(Some(m.info()))
    }

    /// system/profile_full の一覧に載せるモデル情報 (§7)。取り出せないとき (ファイルが消えたなど) は
    /// 一覧全体をエラーにせず `None` にする。一覧は管理画面が数秒ごとに取るので、WARN は出さない
    /// (`container/profile_full`・`container/model_info` を呼べば WARN とエラーが出る)
    pub fn info_for_listing(&self, container_id: &str) -> Option<Value> {
        match self.lock().and_then(|mut inner| {
            let max = inner.max_bytes;
            match inner.models.get_mut(container_id) {
                None => Ok(None),
                Some(m) => m.refresh(max, false).map(|_| Some(m.info())),
            }
        }) {
            Ok(v) => v,
            Err(e) => {
                log::debug!("ContainerModel of Container({container_id}) is not listed: {e:#}");
                None
            }
        }
    }

    /// 中身とモデル情報 (§5.3)。モデルが無ければ `None`。
    /// 読む間に書き換わった (大きさが違う) ら、もう一度だけ覚え直して読み直す。
    /// ファイルを読む間は登録所のロックを外す
    pub fn read(&self, container_id: &str) -> JuizResult<Option<ContainerModelData>> {
        Self::warn_on_error(container_id, self.read_inner(container_id))
    }

    fn read_inner(&self, container_id: &str) -> JuizResult<Option<ContainerModelData>> {
        let mut force = false;
        for _ in 0..2 {
            let (path, file_name, info, size) = {
                let mut inner = self.lock()?;
                let max = inner.max_bytes;
                let Some(m) = inner.models.get_mut(container_id) else {
                    return Ok(None);
                };
                m.refresh(max, force)?;
                (m.path.clone(), m.file_name.clone(), m.info(), m.state.size)
            };
            let data = std::fs::read(&path).map_err(|e| {
                not_found(format!("can not read model file '{file_name}': {e}"))
            })?;
            if data.len() as u64 == size {
                return Ok(Some(ContainerModelData { info, data }));
            }
            force = true;
        }
        Err(invalid(format!(
            "model file of Container({container_id}) keeps changing while being read"
        )))
    }

    /// JSON の経路で `size` バイトのモデルを返す前に呼ぶ。16 MiB 超なら 1 Container につき 1 回 WARN (§5.4)
    pub fn warn_if_large_for_json(&self, container_id: &str, size: u64) {
        if size <= JSON_WARN_BYTES {
            return;
        }
        let Ok(mut inner) = self.lock() else {
            return;
        };
        if inner.json_warned.insert(container_id.to_owned()) {
            log::warn!(
                "ContainerModel of Container({container_id}) is {size} bytes. Serving it through \
                 a JSON broker (base64, 4/3 times larger) is slow; use the HTTP broker \
                 (docs/spec/container-model.md §5.4)"
            );
        }
    }
}

#[cfg(test)]
mod tests;
