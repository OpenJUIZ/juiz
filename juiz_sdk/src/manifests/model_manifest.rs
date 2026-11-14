//! Container の 3D モデル (conf の `model:`) の manifest (docs/spec/container-model.md §5.1、issue #381)

use std::fmt::Display;

use serde::{Deserialize, Serialize};

use crate::geometry::{Quaternion, Transform, Vector3};
use crate::prelude::*;

/// モデルのファイルの形式 (§3.1)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelFormat {
    Glb,
    Gltf,
    Stl,
    Sdf,
    Other,
}

impl ModelFormat {
    /// すべての形式 (conf の `format:` に書ける順)
    pub const ALL: [ModelFormat; 5] = [
        ModelFormat::Glb,
        ModelFormat::Gltf,
        ModelFormat::Stl,
        ModelFormat::Sdf,
        ModelFormat::Other,
    ];

    /// conf の `format:` の名前
    pub fn name(&self) -> &'static str {
        match self {
            ModelFormat::Glb => "glb",
            ModelFormat::Gltf => "gltf",
            ModelFormat::Stl => "stl",
            ModelFormat::Sdf => "sdf",
            ModelFormat::Other => "other",
        }
    }

    /// conf の `format:` の名前から。知らない名前は `None`
    pub fn from_name(name: &str) -> Option<ModelFormat> {
        ModelFormat::ALL.iter().copied().find(|f| f.name() == name)
    }

    /// 拡張子 (`.` なし、大文字小文字は区別しない) から。どれでもなければ [`ModelFormat::Other`]
    pub fn from_extension(ext: &str) -> ModelFormat {
        match ext.to_ascii_lowercase().as_str() {
            "glb" => ModelFormat::Glb,
            "gltf" => ModelFormat::Gltf,
            "stl" => ModelFormat::Stl,
            "sdf" => ModelFormat::Sdf,
            _ => ModelFormat::Other,
        }
    }

    /// パスの拡張子から ([`ModelFormat::from_extension`])
    pub fn from_path(path: &std::path::Path) -> ModelFormat {
        path.extension()
            .and_then(|e| e.to_str())
            .map(ModelFormat::from_extension)
            .unwrap_or(ModelFormat::Other)
    }

    /// Content-Type (§3.1 の `media_type`)
    pub fn media_type(&self) -> &'static str {
        match self {
            ModelFormat::Glb => "model/gltf-binary",
            ModelFormat::Gltf => "model/gltf+json",
            ModelFormat::Stl => "model/stl",
            ModelFormat::Sdf => "application/xml",
            ModelFormat::Other => "application/octet-stream",
        }
    }

    /// 既定の `up_axis` (§4.2)。glTF は `y`、それ以外は `z`
    pub fn default_up_axis(&self) -> &'static str {
        match self {
            ModelFormat::Glb | ModelFormat::Gltf => "y",
            _ => "z",
        }
    }
}

impl Display for ModelFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// conf の `model:` の値 (§5.1)。値の検査は [`ModelManifest::from_value`] がする
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelManifest {
    /// モデルのファイルのパス。相対パスは conf の親ディレクトリからの相対
    pub path: String,
    /// 形式の明示。`None` なら拡張子から決める
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<ModelFormat>,
    /// 1 単位を何 m にするか。`None` なら 1.0
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
    /// Container の座標から見たモデルの取り付け位置 (正規化済み)。`None` なら単位
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Transform>,
    /// ファイルの上の軸 `y` / `z`。`None` なら形式から
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub up_axis: Option<String>,
    /// 色を持たない形式の表示色 (`#RRGGBB` / `#RRGGBBAA`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

impl Serialize for ModelFormat {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

impl<'de> Deserialize<'de> for ModelManifest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(deserializer)?;
        ModelManifest::parse(&v).map_err(|e| serde::de::Error::custom(format!("{e:#}")))
    }
}

fn invalid(message: String) -> anyhow::Error {
    anyhow::anyhow!(JuizError::ContainerModelInvalidError { message })
}

/// `{x, y, z}` (省いたものは 0) を読む
fn parse_vector3(key: &str, v: &Value) -> JuizResult<Vector3> {
    let obj = v
        .as_object()
        .ok_or_else(|| invalid(format!("model.origin.{key} must be a dictionary, but {v}")))?;
    let mut out = [0.0f64; 3];
    for (k, value) in obj.iter() {
        let i = match k.as_str() {
            "x" => 0,
            "y" => 1,
            "z" => 2,
            _ => {
                return Err(invalid(format!(
                    "model.origin.{key} has unknown key '{k}' (x, y, z)"
                )))
            }
        };
        out[i] = value.as_f64().ok_or_else(|| {
            invalid(format!("model.origin.{key}.{k} must be a number, but {value}"))
        })?;
    }
    Ok(Vector3::new(out[0], out[1], out[2]))
}

/// `{x, y, z, w}` (省いたものは x, y, z が 0、w が 1) を読む
fn parse_quaternion(key: &str, v: &Value) -> JuizResult<Quaternion> {
    let obj = v
        .as_object()
        .ok_or_else(|| invalid(format!("model.origin.{key} must be a dictionary, but {v}")))?;
    let mut out = [0.0f64, 0.0, 0.0, 1.0];
    for (k, value) in obj.iter() {
        let i = match k.as_str() {
            "x" => 0,
            "y" => 1,
            "z" => 2,
            "w" => 3,
            _ => {
                return Err(invalid(format!(
                    "model.origin.{key} has unknown key '{k}' (x, y, z, w)"
                )))
            }
        };
        out[i] = value.as_f64().ok_or_else(|| {
            invalid(format!("model.origin.{key}.{k} must be a number, but {value}"))
        })?;
    }
    Ok(Quaternion::new(out[0], out[1], out[2], out[3]))
}

/// `#RRGGBB` か `#RRGGBBAA`
fn is_color(s: &str) -> bool {
    let Some(hex) = s.strip_prefix('#') else {
        return false;
    };
    (hex.len() == 6 || hex.len() == 8) && hex.chars().all(|c| c.is_ascii_hexdigit())
}

impl ModelManifest {
    /// `model:` に書けるキー (§5.1)
    pub const CONF_KEYS: [&'static str; 6] = ["path", "format", "scale", "origin", "up_axis", "color"];

    pub fn new(path: &str) -> Self {
        Self {
            path: path.to_owned(),
            format: None,
            scale: None,
            origin: None,
            up_axis: None,
            color: None,
        }
    }

    /// conf の `model:` の値を読む (§5.1)。型・値の誤りは `ContainerModelInvalidError`。
    /// 表に無いキーは警告を出して使わない
    pub fn from_value(v: &Value) -> JuizResult<Self> {
        let m = Self::parse(v)?;
        let unknown = Self::unknown_keys(v);
        if !unknown.is_empty() {
            log::warn!(
                "unknown key(s) {} in model (path='{}') are ignored. Known keys: {}",
                unknown.join(", "),
                m.path,
                Self::CONF_KEYS.join(", ")
            );
        }
        Ok(m)
    }

    /// `model:` の表に無いキー
    pub fn unknown_keys(v: &Value) -> Vec<String> {
        v.as_object()
            .map(|o| {
                o.keys()
                    .filter(|k| !Self::CONF_KEYS.contains(&k.as_str()))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// [`ModelManifest::from_value`] の、警告を出さないもの
    fn parse(v: &Value) -> JuizResult<Self> {
        let obj = v
            .as_object()
            .ok_or_else(|| invalid(format!("model must be a dictionary, but {v}")))?;
        let path = match obj.get("path") {
            Some(Value::String(s)) if !s.is_empty() => s.clone(),
            Some(other) => {
                return Err(invalid(format!(
                    "model.path must be a non-empty string, but {other}"
                )))
            }
            None => return Err(invalid("model.path is required".to_owned())),
        };
        let mut m = ModelManifest::new(&path);
        if let Some(f) = obj.get("format").filter(|f| !f.is_null()) {
            let name = f
                .as_str()
                .ok_or_else(|| invalid(format!("model.format must be a string, but {f}")))?;
            m.format = Some(ModelFormat::from_name(name).ok_or_else(|| {
                invalid(format!(
                    "model.format must be one of glb, gltf, stl, sdf, other, but '{name}'"
                ))
            })?);
        }
        if let Some(s) = obj.get("scale").filter(|s| !s.is_null()) {
            let scale = s
                .as_f64()
                .ok_or_else(|| invalid(format!("model.scale must be a number, but {s}")))?;
            if !(scale.is_finite() && scale > 0.0) {
                return Err(invalid(format!(
                    "model.scale must be a positive finite number, but {scale}"
                )));
            }
            m.scale = Some(scale);
        }
        if let Some(o) = obj.get("origin").filter(|o| !o.is_null()) {
            let oo = o
                .as_object()
                .ok_or_else(|| invalid(format!("model.origin must be a dictionary, but {o}")))?;
            let mut t = Transform::default();
            for (k, value) in oo.iter() {
                match k.as_str() {
                    "translation" => t.translation = parse_vector3(k, value)?,
                    "rotation" => t.rotation = parse_quaternion(k, value)?,
                    _ => {
                        return Err(invalid(format!(
                            "model.origin has unknown key '{k}' (translation, rotation)"
                        )))
                    }
                }
            }
            m.origin = Some(
                t.normalized()
                    .map_err(|e| invalid(format!("model.origin is invalid: {e}")))?,
            );
        }
        if let Some(a) = obj.get("up_axis").filter(|a| !a.is_null()) {
            match a.as_str() {
                Some(s @ ("y" | "z")) => m.up_axis = Some(s.to_owned()),
                _ => {
                    return Err(invalid(format!(
                        "model.up_axis must be 'y' or 'z', but {a}"
                    )))
                }
            }
        }
        if let Some(c) = obj.get("color").filter(|c| !c.is_null()) {
            match c.as_str() {
                Some(s) if is_color(s) => m.color = Some(s.to_owned()),
                _ => {
                    return Err(invalid(format!(
                        "model.color must be \"#RRGGBB\" or \"#RRGGBBAA\", but {c}"
                    )))
                }
            }
        }
        Ok(m)
    }

    pub fn format(mut self, format: ModelFormat) -> Self {
        self.format = Some(format);
        self
    }

    pub fn scale(mut self, scale: f64) -> Self {
        self.scale = Some(scale);
        self
    }

    pub fn origin(mut self, origin: Transform) -> Self {
        self.origin = Some(origin);
        self
    }

    pub fn up_axis(mut self, up_axis: &str) -> Self {
        self.up_axis = Some(up_axis.to_owned());
        self
    }

    pub fn color(mut self, color: &str) -> Self {
        self.color = Some(color.to_owned());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err_message(v: Value) -> String {
        let e = ModelManifest::from_value(&v).unwrap_err();
        assert!(
            matches!(
                e.downcast_ref::<JuizError>(),
                Some(JuizError::ContainerModelInvalidError { .. })
            ),
            "{e:#}"
        );
        format!("{e:#}")
    }

    #[test]
    fn reads_all_keys() {
        let m = ModelManifest::from_value(&jvalue!({
            "path": "models/camera.stl",
            "format": "stl",
            "scale": 0.001,
            "origin": {"translation": {"z": -0.02}, "rotation": {"x": 0, "y": 0, "z": 0, "w": 2}},
            "up_axis": "z",
            "color": "#404040",
        }))
        .unwrap();
        assert_eq!(m.path, "models/camera.stl");
        assert_eq!(m.format, Some(ModelFormat::Stl));
        assert_eq!(m.scale, Some(0.001));
        let o = m.origin.unwrap();
        assert_eq!(o.translation, Vector3::new(0.0, 0.0, -0.02));
        // 保存するときに正規化する
        assert_eq!(o.rotation, Quaternion::new(0.0, 0.0, 0.0, 1.0));
        assert_eq!(m.up_axis.as_deref(), Some("z"));
        assert_eq!(m.color.as_deref(), Some("#404040"));
        // 直列化して読み直しても同じ
        let back: ModelManifest = serde_json::from_value(serde_json::to_value(&m).unwrap()).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn only_path_is_required() {
        let m = ModelManifest::from_value(&jvalue!({"path": "a.glb"})).unwrap();
        assert_eq!(m, ModelManifest::new("a.glb"));
        assert_eq!(serde_json::to_value(&m).unwrap(), jvalue!({"path": "a.glb"}));
        assert!(err_message(jvalue!({})).contains("path"));
        assert!(err_message(jvalue!({"path": ""})).contains("path"));
        assert!(err_message(jvalue!("a.glb")).contains("dictionary"));
    }

    #[test]
    fn rejects_invalid_values() {
        for v in [
            jvalue!({"path": "a.glb", "format": "obj"}),
            jvalue!({"path": "a.glb", "scale": 0}),
            jvalue!({"path": "a.glb", "scale": -1.0}),
            jvalue!({"path": "a.glb", "scale": "1"}),
            jvalue!({"path": "a.glb", "up_axis": "x"}),
            jvalue!({"path": "a.glb", "color": "red"}),
            jvalue!({"path": "a.glb", "color": "#12345"}),
            jvalue!({"path": "a.glb", "origin": {"rotation": {"x": 0, "y": 0, "z": 0, "w": 0}}}),
            jvalue!({"path": "a.glb", "origin": {"translation": {"x": "1"}}}),
            jvalue!({"path": "a.glb", "origin": {"position": {"x": 1}}}),
        ] {
            err_message(v);
        }
    }

    #[test]
    fn unknown_keys_are_listed() {
        let v = jvalue!({"path": "a.glb", "scael": 0.001});
        assert_eq!(ModelManifest::unknown_keys(&v), vec!["scael".to_owned()]);
        assert!(ModelManifest::from_value(&v).is_ok());
    }

    #[test]
    fn formats() {
        assert_eq!(ModelFormat::from_extension("GLB"), ModelFormat::Glb);
        assert_eq!(ModelFormat::from_extension("obj"), ModelFormat::Other);
        assert_eq!(
            ModelFormat::from_path(std::path::Path::new("x/robot.urdf")),
            ModelFormat::Other
        );
        assert_eq!(ModelFormat::Glb.default_up_axis(), "y");
        assert_eq!(ModelFormat::Stl.default_up_axis(), "z");
        assert_eq!(ModelFormat::Sdf.media_type(), "application/xml");
        for f in ModelFormat::ALL {
            assert_eq!(ModelFormat::from_name(f.name()), Some(f));
        }
    }
}
