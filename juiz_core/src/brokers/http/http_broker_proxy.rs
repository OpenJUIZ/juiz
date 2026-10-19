use std::{
    collections::HashMap,
    io::Read,
    sync::{Arc, Mutex},
};

use juiz_sdk::anyhow;
use juiz_sdk::connection_identifier::ConnectionIdentifier;

use crate::brokers::{create_broker_proxy_factory_impl, BrokerProxy, BrokerProxyFactory};
use crate::{core::CoreWorker, prelude::*};

//use reqwest::Response;
use crate::brokers::{CRUDBrokerProxy, CRUDBrokerProxyHolder};
use reqwest::blocking::Response;
use thiserror::Error;

// #[cfg(feature="opencv4")]
// use opencv::imgcodecs::*;

#[derive(Error, Debug, PartialEq)]
enum HTTPBrokerError {
    #[error("General Error")]
    GeneralError {},

    #[error("HTTPStatusError (status_code={status_code:}), message_body={message}")]
    HTTPStatusError {
        status_code: reqwest::StatusCode,
        message: String,
    },
}

fn name_to_host_and_port<'a>(name: &'a String) -> JuizResult<(&'a str, i64)> {
    let mut tokens = name.split(':');
    let host = tokens.next();
    if host.is_none() {
        return Err(anyhow::Error::from(
            JuizError::BrokerNameCanNotResolveToURLError {
                given_name: name.clone(),
            },
        ));
    }
    let port = tokens.next();
    if port.is_none() {
        return Ok((host.unwrap(), 8080));
    }
    Ok((host.unwrap(), port.unwrap().parse()?))
}

struct HTTPBrokerProxy {
    base_url: String,
    /// 接続（keep-alive）を使い回すため、呼び出しごとに作らずこの 1 つを使う
    client: reqwest::blocking::Client,
}

impl HTTPBrokerProxy {
    pub fn new(manifest: &Value) -> JuizResult<HTTPBrokerProxy> {
        log::trace!("new({manifest:}) called");
        let name = obj_get_str(manifest, "name")?.to_string();
        let (addr, port) = name_to_host_and_port(&name)?;
        Ok(HTTPBrokerProxy {
            client: reqwest::blocking::Client::new(),
            base_url: "http://".to_string() + addr + ":" + i64::to_string(&port).as_str() + "/api",
        })
    }
}

fn construct_param(key: &String, value: &String) -> String {
    if key == "identifier" && value.starts_with("connection://") {
        // コネクションの識別子は両端のプロセスの識別子をそれぞれ書き換える (issue #19)
        match ConnectionIdentifier::try_from(value.clone()) {
            Ok(mut con_id) => {
                for id in [
                    &mut con_id.source_identifier,
                    &mut con_id.destination_identifier,
                ] {
                    id.broker_type_name = "core".to_owned();
                    id.broker_name = "core".to_owned();
                }
                let new_id: String = con_id.into();
                key.clone() + "=" + new_id.as_str()
            }
            Err(_) => key.clone() + "=" + value.as_str(),
        }
    } else if key == "identifier" {
        // 解析できない識別子はそのまま送り、サーバ側でエラーにする (issue #35)
        match IdentifierStruct::try_from(value.clone()) {
            Ok(mut id_struct) => {
                id_struct.broker_type_name = "core".to_owned();
                id_struct.broker_name = "core".to_owned();
                let new_id: String = id_struct.into();
                key.clone() + "=" + new_id.as_str()
            }
            Err(_) => key.clone() + "=" + value.as_str(),
        }
    } else {
        (key).clone() + "=" + (value).as_str()
    }
}

fn construct_url(
    base_url: &String,
    class_name: &str,
    function_name: &str,
    param: &HashMap<String, String>,
) -> String {
    let url = base_url.clone() + "/" + class_name + "/" + function_name;
    if param.len() == 0 {
        return url;
    }
    let m = param.iter().map(|(k, v)| construct_param(k, v));
    return url + "?" + m.collect::<Vec<String>>().join("&").as_str();
}

impl CRUDBrokerProxy for HTTPBrokerProxy {
    fn create(
        &self,
        class_name: &str,
        function_name: &str,
        payload: Value,
        param: std::collections::HashMap<String, String>,
    ) -> JuizResult<CapsulePtr> {
        log::trace!("【呼出】HTTPBrokerProxy({}).create({class_name:}, {function_name}, {payload}, {param:?})", self.base_url);
        match self
            .client
            .post(construct_url(
                &self.base_url,
                class_name,
                function_name,
                &param,
            ))
            .json(&payload)
            .send()
        {
            Err(e) => Err(anyhow::Error::from(e)),
            Ok(response) => {
                let status_code = response.status();
                if status_code != 200 {
                    log::error!("HTTPBrokerProxy.create() failed. Response is {response:?}");
                    return Err(anyhow::Error::from(HTTPBrokerError::HTTPStatusError {
                        status_code,
                        message: response.text().unwrap_or_default(),
                    }));
                }
                Ok(response
                    .json::<Value>()
                    .map_err(|e| anyhow::Error::from(e))?
                    .into())
            }
        }
    }

    fn delete(
        &self,
        class_name: &str,
        function_name: &str,
        param: std::collections::HashMap<String, String>,
    ) -> JuizResult<CapsulePtr> {
        log::trace!(
            "【呼出】HTTPBrokerProxy({}).delete({class_name:}, {function_name}, {param:?})",
            self.base_url
        );
        match self
            .client
            .delete(construct_url(
                &self.base_url,
                class_name,
                function_name,
                &param,
            ))
            .send()
        {
            Err(e) => Err(anyhow::Error::from(e)),
            Ok(response) => {
                let status_code = response.status();
                if status_code != 200 {
                    log::error!("HTTPBrokerProxy.delete() failed. Response is {response:?}");
                    return Err(anyhow::Error::from(HTTPBrokerError::HTTPStatusError {
                        status_code,
                        message: response.text().unwrap_or_default(),
                    }));
                }
                Ok(response
                    .json::<Value>()
                    .map_err(|e| anyhow::Error::from(e))?
                    .into())
            }
        }
    }

    fn read(
        &self,
        class_name: &str,
        function_name: &str,
        param: std::collections::HashMap<String, String>,
    ) -> JuizResult<CapsulePtr> {
        log::trace!(
            "【呼出】HTTPBrokerProxy({}).read({class_name:}, {function_name}, {param:?})",
            self.base_url
        );

        let url = construct_url(&self.base_url, class_name, function_name, &param);
        log::trace!("【read】(url={url:})に対してリクエストをします。");
        match self.client.get(url.clone()).send() {
            Err(e) => Err(anyhow::Error::from(e)),
            Ok(response) => {
                if response.status() != 200 {
                    log::error!(
                        "HTTPBrokerProxy.read(url={url:})が失敗しました。Responseは{response:?}"
                    );
                    return Err(anyhow::Error::from(HTTPBrokerError::HTTPStatusError {
                        status_code: response.status(),
                        message: format!("{:?}", response),
                    }));
                }
                let value = response
                    .json::<Value>()
                    .map_err(|e| anyhow::Error::from(e))?;
                log::debug!("【read】Responseは{value:}");
                let return_value = Ok(value.into());
                //log::trace!("HTTPBrokerProxy.read({}) returns {return_value:?}", self.base_url);
                return_value
            }
        }
    }

    fn update(
        &self,
        class_name: &str,
        function_name: &str,
        payload: CapsuleMap,
        param: std::collections::HashMap<String, String>,
    ) -> JuizResult<CapsulePtr> {
        log::trace!("【呼出】HTTPBrokerProxy({}).update({class_name:}, {function_name}, {payload}, {param:?})", self.base_url);
        let v: Value = payload.into();
        match self
            .client
            .patch(construct_url(
                &self.base_url,
                class_name,
                function_name,
                &param,
            ))
            .json(&v)
            .send()
        {
            Err(e) => {
                log::error!("HttpBrokerProxy::update() failed. Error is {:?}", e);
                Err(anyhow::Error::from(e))
            }
            Ok(response) => {
                let status_code = response.status();
                if status_code != 200 {
                    log::error!("HTTPBrokerProxy.update() failed. Response is {response:?}");
                    return Err(anyhow::Error::from(HTTPBrokerError::HTTPStatusError {
                        status_code,
                        message: response.text().unwrap_or_default(),
                    }));
                }
                let hdr = response.headers();
                if hdr["content-type"] == "image/png" {
                    image_png_response_to_capsule_ptr(response)
                } else {
                    match response.json::<Value>() {
                        Err(e) => Err(anyhow::Error::from(e)),
                        Ok(v) => Ok(v.into()),
                    }
                }
            }
        }
    }
}

// #[cfg(feature="opencv4")]
// fn image_png_response_to_capsule_ptr(mut response: Response) -> JuizResult<CapsulePtr> {
//         let mut buf: Vec<u8> = Vec::new();
//         let _result = response.read_to_end(&mut buf)?;
//         Ok(imdecode(&opencv::core::Vector::<u8>::from_iter(buf), IMREAD_COLOR)?.into())

// }

// #[cfg(not(feature="opencv4"))]
fn image_png_response_to_capsule_ptr(mut response: Response) -> JuizResult<CapsulePtr> {
    let mut buf: Vec<u8> = Vec::new();
    let _result = response.read_to_end(&mut buf)?;
    let image = juiz_sdk::image::load_from_memory(buf.as_ref())?;
    Ok(image.into())
}

fn create_broker_proxy_function(
    _core_broker: &CoreWorker,
    manifest: Value,
) -> JuizResult<Arc<Mutex<dyn BrokerProxy>>> {
    let name = obj_get_str(&manifest, "name")?;
    Ok(CRUDBrokerProxyHolder::new(
        "HTTPBrokerProxy",
        "http",
        name,
        Box::new(HTTPBrokerProxy::new(&manifest)?),
    )?)
}

pub fn http_broker_proxy_factory() -> JuizResult<Arc<Mutex<dyn BrokerProxyFactory>>> {
    let manifest = jvalue!({
        "type_name": "http"
    });
    create_broker_proxy_factory_impl(manifest, create_broker_proxy_function)
}
