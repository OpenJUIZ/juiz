//! System をまたぐ Transform の、別 System への回し方 (docs/spec/transformations.md §19.2・§19.8、Step 2 #415)
//!
//! - `transform/create` の終点が別 System の Container なら、終点の System の `transform/create` に回す。
//!   始点は自分の core の ID、`source_system` に自分の UUID を付ける。受けた System は、その UUID の
//!   System (マスターシステム・サブシステム) へのブローカーで始点を書き換えて作る (§19.2 の 5)。
//! - `transform/profile_full`・`update`・`destroy` も、ID の終点が別 System なら回す (§19.2 の 7)。
//! - `container/pose`・`set_pose` の `identifier` が別 System の Container なら、その System に回し、
//!   基準は `reference_system` (自分の UUID) で相手の写しから探してもらう (§19.8)。
//!
//! 回す呼び出しは CoreBroker の読み出しのロックを持ったまま相手を呼ぶ。相手は回された処理の中で
//! こちらを呼び返さない (写しの取り込みは相手の別スレッド) ので、ロックが絡んで止まることはない。

use super::*;
use juiz_sdk::manifests::transform_identifier;

/// ブローカー付きの、別 System の Container の ID なら `Some` (§19.2 の 2)
pub(crate) fn remote_container_id(s: &str) -> Option<ContainerIdentifier> {
    if !s.contains("://") {
        return None;
    }
    let id = ContainerIdentifier::try_from(s.to_owned()).ok()?;
    if id.broker_type_name == "core" || id.class_name != "container" {
        return None;
    }
    Some(id)
}

fn core_form(id: &ContainerIdentifier) -> ContainerIdentifier {
    let mut c = id.clone();
    c.broker_type_name = "core".to_owned();
    c.broker_name = "core".to_owned();
    c
}

fn cross_error(message: String) -> juiz_sdk::anyhow::Error {
    anyhow!(JuizError::CrossSystemTransformError { message })
}

/// `transform://<始点>|<終点>` を始点と終点に分ける
fn split_transform_id(id: &str) -> Option<(String, String)> {
    let rest = id.strip_prefix("transform://")?;
    let (s, d) = rest.split_once('|')?;
    Some((s.to_owned(), d.to_owned()))
}

/// 回した先のエラー。相手に届かなかった (juiz のエラーでない) ものは `CrossSystemTransformError` にする (§19.2 の 6)
fn forward_error(e: juiz_sdk::anyhow::Error, target: &ContainerIdentifier) -> juiz_sdk::anyhow::Error {
    if e.downcast_ref::<JuizError>().is_some() {
        return e.context(format!(
            "in the System of Container({target}) (§19.2, §19.8)"
        ));
    }
    cross_error(format!(
        "can not reach the System of Container({target}): {e:#}"
    ))
}

/// 回した先の答えの `frame_id` (相手の写しの ID) を、自分の System での基準の名前にする (§5.4)
fn with_local_frame(mut p: PoseStamped, reference_id: &str) -> PoseStamped {
    if let Ok(r) = ContainerIdentifier::try_from(reference_id.to_owned()) {
        p.header.frame_id = r.full_name();
    }
    p
}

impl CoreBroker {
    fn own_uuid(&self) -> JuizResult<String> {
        Ok(self.system_store.uuid()?.to_string())
    }

    /// UUID の System (マスターシステム・サブシステム) へのブローカーの (種類, 名前) (§19.2 の 5)
    fn broker_of_system(&self, uuid: &str) -> JuizResult<(String, String)> {
        for ssp in self
            .master_system_proxy
            .iter()
            .chain(self.subsystem_proxies.iter())
        {
            if ssp.uuid().to_string() == uuid {
                let bp = ssp.broker_proxy();
                let b = juiz_borrow(&bp)?;
                return Ok((b.type_name().to_owned(), b.name().to_owned()));
            }
        }
        Err(cross_error(format!(
            "the System (uuid={uuid}) is not connected to this System \
             (neither the mastersystem nor a subsystem)."
        )))
    }

    /// 別 System `uuid` から回された、その System での core の ID を、その System へのブローカー付きの ID にする
    fn rewrite_from_system(&self, id: &str, uuid: &str) -> JuizResult<String> {
        let mut c = ContainerIdentifier::try_from(id.to_owned())?;
        if c.broker_type_name != "core" {
            return Err(cross_error(format!(
                "source ({id}) with 'source_system' must be the core identifier in that System."
            )));
        }
        let (t, n) = self.broker_of_system(uuid)?;
        c.broker_type_name = t;
        c.broker_name = n;
        Ok(c.to_string())
    }

    fn proxy_of(&self, id: &ContainerIdentifier) -> JuizResult<Arc<RwLock<dyn BrokerProxy>>> {
        self.worker()
            .broker_proxy(&id.broker_type_name, &id.broker_name, true)
            .map_err(|e| {
                cross_error(format!(
                    "can not make the BrokerProxy to the System of Container({id}): {e:#}"
                ))
            })
    }

    /// Transform を作る (§19.2)。名前は `namespace` から解決する (conf の `transforms:` は System の既定ネームスペース)
    pub(crate) fn transform_create_in(
        &self,
        manifest: &TransformManifest,
        namespace: &str,
    ) -> JuizResult<Value> {
        // 別 System から回されたもの (§19.2 の 5)
        if let Some(uuid) = manifest.source_system.as_deref() {
            let mut m = manifest.clone();
            m.source = self.rewrite_from_system(&manifest.source, uuid)?;
            m.source_system = None;
            return self.create_local_transform(&m, namespace);
        }
        // 終点が別 System の Container: 終点の System に回す (§19.2 の 5・6)
        if let Some(dst) = remote_container_id(&manifest.destination) {
            if remote_container_id(&manifest.source).is_some() {
                return Err(cross_error(format!(
                    "both source ({}) and destination ({}) are Containers of other Systems. \
                     Create the Transform in the System of its destination.",
                    manifest.source, manifest.destination
                )));
            }
            let mut m = manifest.clone();
            m.source = self
                .transformations()
                .resolve_local_id(&manifest.source, namespace)?;
            m.destination = core_form(&dst).to_string();
            m.source_system = Some(self.own_uuid()?);
            let bp = self.proxy_of(&dst)?;
            let result = juiz_borrow(&bp)?.transform_create(&m);
            return result.map_err(|e| forward_error(e, &dst));
        }
        self.create_local_transform(manifest, namespace)
    }

    fn create_local_transform(
        &self,
        manifest: &TransformManifest,
        namespace: &str,
    ) -> JuizResult<Value> {
        let mut profiles = self
            .transformations()
            .create_transforms(std::slice::from_ref(manifest), namespace)?;
        profiles.pop().ok_or_else(|| {
            anyhow!(JuizError::InvalidValueError {
                message: "transform_create created nothing".to_owned()
            })
        })
    }

    /// Transform の ID を、この System の登録所の ID にする。
    /// `source_system` があれば始点をその System へのブローカー付きの ID に書き換える (§19.2 の 7)
    fn local_transform_id(&self, identifier: &str, source_system: Option<&str>) -> JuizResult<String> {
        let Some(uuid) = source_system else {
            return Ok(identifier.to_owned());
        };
        let (s, d) = split_transform_id(identifier).ok_or_else(|| {
            anyhow!(JuizError::TransformNotFoundError {
                identifier: identifier.to_owned()
            })
        })?;
        Ok(transform_identifier(&self.rewrite_from_system(&s, uuid)?, &d))
    }

    /// ID の終点が別 System の Container なら、その System の BrokerProxy と、回すときの ID・付ける UUID (§19.2 の 7)
    fn forwarded_transform(
        &self,
        identifier: &str,
    ) -> JuizResult<Option<(Arc<RwLock<dyn BrokerProxy>>, ContainerIdentifier, String, String)>> {
        let Some((s, d)) = split_transform_id(identifier) else {
            return Ok(None);
        };
        let Some(dst) = remote_container_id(&d) else {
            return Ok(None);
        };
        if remote_container_id(&s).is_some() {
            return Err(cross_error(format!(
                "both ends of Transform ({identifier}) are Containers of other Systems."
            )));
        }
        let src = self.transformations().resolve_local_id(&s, "")?;
        let fid = transform_identifier(&src, &core_form(&dst).to_string());
        Ok(Some((self.proxy_of(&dst)?, dst, fid, self.own_uuid()?)))
    }

    /// Transform の読み書きを、自分の登録所か終点の System で行う (§19.2 の 7)
    pub(super) fn with_transform<T>(
        &self,
        identifier: &str,
        source_system: Option<&str>,
        local: impl FnOnce(&crate::transformations::Transformations, &str) -> JuizResult<T>,
        remote: impl FnOnce(&dyn BrokerProxy, &str, &str) -> JuizResult<T>,
    ) -> JuizResult<T> {
        if source_system.is_none() {
            if let Some((bp, dst, fid, uuid)) = self.forwarded_transform(identifier)? {
                let result = remote(&*juiz_borrow(&bp)?, &fid, &uuid);
                return result.map_err(|e| forward_error(e, &dst));
            }
        }
        let id = self.local_transform_id(identifier, source_system)?;
        local(self.transformations(), &id)
    }

    /// `container/pose` (§7.1・§19.8)
    pub(super) fn pose_across(
        &self,
        identifier: &str,
        reference: &str,
        reference_system: Option<&str>,
    ) -> JuizResult<PoseStamped> {
        let t = self.transformations();
        if let Some(uuid) = reference_system {
            return t.pose_by_name_from_system(identifier, reference, "", uuid);
        }
        if !t.knows(identifier, "")? {
            if let Some(c) = remote_container_id(identifier) {
                // C は別 System: C の System に回し、基準 R はそこで写しから探してもらう
                let r = self.local_reference(reference)?;
                let bp = self.proxy_of(&c)?;
                let result = juiz_borrow(&bp)?.container_pose_from_system(
                    &core_form(&c).to_string(),
                    &r,
                    Some(&self.own_uuid()?),
                );
                return result
                    .map(|p| with_local_frame(p, &r))
                    .map_err(|e| forward_error(e, &c));
            }
        }
        if !t.knows(reference, "")? {
            if let Some(r) = remote_container_id(reference) {
                // R は別 System、C は自分の Container: R の System に pose(R, C) を頼み、逆にする
                let c = t.resolve_local_id(identifier, "")?;
                let bp = self.proxy_of(&r)?;
                let result = juiz_borrow(&bp)?.container_pose_from_system(
                    &core_form(&r).to_string(),
                    &c,
                    Some(&self.own_uuid()?),
                );
                let p = result.map_err(|e| forward_error(e, &r))?;
                return Ok(PoseStamped::new(
                    &r.to_string(),
                    p.header.stamp,
                    p.pose.inverse(),
                ));
            }
        }
        t.pose_by_name(identifier, reference, "")
    }

    /// `container/set_pose` (§7.2・§19.8)
    pub(super) fn set_pose_across(
        &self,
        identifier: &str,
        reference: &str,
        reference_system: Option<&str>,
        pose: &Pose,
    ) -> JuizResult<PoseStamped> {
        let t = self.transformations();
        if let Some(uuid) = reference_system {
            return t.set_pose_by_name_from_system(identifier, reference, "", uuid, pose);
        }
        if !t.knows(identifier, "")? {
            if let Some(c) = remote_container_id(identifier) {
                let r = self.local_reference(reference)?;
                let bp = self.proxy_of(&c)?;
                let result = juiz_borrow(&bp)?.container_set_pose_from_system(
                    &core_form(&c).to_string(),
                    &r,
                    Some(&self.own_uuid()?),
                    pose,
                );
                return result
                    .map(|p| with_local_frame(p, &r))
                    .map_err(|e| forward_error(e, &c));
            }
        }
        if !t.knows(reference, "")? && remote_container_id(reference).is_some() {
            return Err(cross_error(format!(
                "set_pose with a reference ({reference}) of another System is not supported. \
                 Write it in the System of Container({identifier}) with a reference in that System (§19.8)."
            )));
        }
        t.set_pose_by_name(identifier, reference, "", pose)
    }

    /// 回すときの基準。自分の System の Container でなければならない (§19.8)
    fn local_reference(&self, reference: &str) -> JuizResult<String> {
        self.transformations()
            .resolve_local_id(reference, "")
            .map_err(|e| {
                e.context(format!(
                    "the reference ({reference}) must be a Container of this System \
                     when the Container is of another System (§19.8)."
                ))
            })
    }
}
