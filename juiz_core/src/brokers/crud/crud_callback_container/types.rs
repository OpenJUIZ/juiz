//! 型の登録所の API (issue #193)
//!
//! - `type/list`: 登録している名前付き型 (`TypeEntry` の配列)
//! - `type/schema?identifier=<完全名>@sha256:…`: 型参照 `{name, fingerprint, avro}`。完全名だけでもよい
use crate::brokers::TypeBrokerProxy;
use juiz_sdk::{anyhow::anyhow, log, prelude::*};

use super::CallbackContainerType;

pub(crate) fn create_read_callbacks() -> CallbackContainerType {
    let mut cbs = CallbackContainerType::new();
    cbs.insert("list", |_crud, cb, _args| {
        log::debug!("[READ  ] type/list called");
        Ok(serde_json::to_value(cb.lock()?.type_list()?)?.into())
    });
    cbs.insert("schema", |_crud, cb, args| {
        log::debug!("[READ  ] type/schema called");
        let identifier = args.get_param("identifier").cloned().ok_or_else(|| {
            anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError {
                key_name: "identifier".to_owned()
            })
        })?;
        Ok(serde_json::to_value(cb.lock()?.type_schema(&identifier)?)?.into())
    });
    cbs
}
