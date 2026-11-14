//! Container の 3D モデルの取り出し (docs/spec/container-model.md §6、issue #381)
//!
//! - `identifier` は Container の ID か名前 (名前は根のネームスペースから、Transformations と同じ解決の規則)。
//!   名前が自分の System に無く、別 System の Container の写し (§19.5) に当たれば、その System に回す。
//!   パスは受け付けない (§6.1)。
//! - 別 System の Container (ブローカー付きの ID) なら、その System に回す。中継する System はモデルを持たず、
//!   受け取ったものをそのまま渡す (§6.1)。

use super::cross_system::{core_form, proxy_error_context, remote_container_id};
use super::*;
use crate::models::ContainerModelData;

/// モデルを取り出す先
enum ModelTarget {
    /// この System の Container (core の ID の文字列)
    Local(String),
    /// 別 System の Container
    Remote(ContainerIdentifier),
}

fn no_model(container: &str) -> juiz_sdk::anyhow::Error {
    anyhow!(JuizError::ContainerHasNoModelError {
        container: container.to_owned()
    })
}

impl CoreBroker {
    fn model_target(&self, identifier: &str) -> JuizResult<ModelTarget> {
        let e = match self.transformations().resolve_local_id(identifier, "") {
            Ok(id) => return Ok(ModelTarget::Local(id)),
            Err(e) => e,
        };
        if let Some(c) = remote_container_id(identifier) {
            return Ok(ModelTarget::Remote(c));
        }
        // 名前が別 System の Container の写しに当たれば、その System に回す (§6.1、写しの ID はブローカー付き)
        match self.transformations().resolve_replica_id(identifier, "") {
            Ok(id) => remote_container_id(&id).map(ModelTarget::Remote).ok_or(e),
            Err(_) => Err(e),
        }
    }

    /// `container/model_info` (§6.1)。モデルが無ければ `Null`
    pub(crate) fn container_model_info_impl(&self, identifier: &str) -> JuizResult<Value> {
        log::trace!("【呼出】container_model_info({identifier})");
        match self.model_target(identifier)? {
            ModelTarget::Local(id) => Ok(self
                .worker()
                .store()
                .models
                .info(&id)?
                .unwrap_or(Value::Null)),
            ModelTarget::Remote(c) => {
                let bp = self.proxy_of(&c)?;
                let r = juiz_borrow(&bp)?.container_model_info(&core_form(&c).to_string());
                r.map_err(|e| proxy_error_context(e, &c))
            }
        }
    }

    /// `container/model` の中身とモデル情報 (§6.1)。モデルが無ければ `ContainerHasNoModelError`。
    /// HTTP の生のバイト列の経路 (§6.2) はこれを直接使う
    pub(crate) fn container_model_data(&self, identifier: &str) -> JuizResult<ContainerModelData> {
        log::trace!("【呼出】container_model({identifier})");
        match self.model_target(identifier)? {
            ModelTarget::Local(id) => self
                .worker()
                .store()
                .models
                .read(&id)?
                .ok_or_else(|| no_model(&id)),
            ModelTarget::Remote(c) => {
                let bp = self.proxy_of(&c)?;
                let v = juiz_borrow(&bp)?
                    .container_model(&core_form(&c).to_string())
                    .map_err(|e| proxy_error_context(e, &c))?;
                ContainerModelData::from_value(&v)
            }
        }
    }

    /// `container/model` の JSON の経路の形 `{info, data: <base64>}` (§6.3)。16 MiB 超なら WARN (§5.4)
    pub(crate) fn container_model_json(&self, identifier: &str) -> JuizResult<Value> {
        let d = self.container_model_data(identifier)?;
        self.worker()
            .store()
            .models
            .warn_if_large_for_json(identifier, d.data.len() as u64);
        Ok(d.to_value())
    }
}
