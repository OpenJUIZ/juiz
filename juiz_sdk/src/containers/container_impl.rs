use crate::{
    geometry::{Pose, PoseStamped},
    manifests::ContainerProfile,
    prelude::*,
    processes::ProcessPtr,
    transformations::{ContainerPoseHandle, ContainerPoseReader, PoseStatePtr},
};
///
///
///
use std::{
    collections::HashMap,
    fmt::Display,
    ops::{Deref, DerefMut},
};

#[allow(unused)]
pub struct ContainerImpl<S: 'static> {
    // core: ObjectCore,
    profile: ContainerProfile,
    identifier: ContainerIdentifier,
    pub t: Box<S>,
    processes: HashMap<String, ProcessPtr>,
    parent_container: Option<ContainerPtr>,
    /// ContainerPose (利用者の状態とは別のロック、docs/spec/transformations.md §9)
    pose: ContainerPoseHandle,
}

impl<S: 'static> std::fmt::Debug for ContainerImpl<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContainerImpl")
            .field("profile", &self.profile)
            .field("identifier", &self.identifier)
            .field("t", &"User object.".to_owned())
            .field("processes", &self.processes)
            .field("parent_container", &self.parent_container)
            .field("pose", &self.pose)
            .finish()
    }
}

fn _identifier_from_manifest(manifest: &Value) -> Identifier {
    match obj_get_str(manifest, "identifier") {
        Err(_) => obj_get_str(manifest, "name").unwrap().to_string(),
        Ok(id) => id.to_string(),
    }
}

impl<S: 'static> ContainerImpl<S> {
    pub fn new(manifest: ContainerManifest, t: Box<S>) -> JuizResult<Self> {
        //println!("new(manifest:{manifest:?}");
        let identifier = manifest.identifier()?;
        Ok(ContainerImpl {
            // core: ObjectCore::create(JuizObjectClass::Container("ContainerImpl"), manifest.type_name.to_owned(), manifest.name.as_ref().unwrap().to_owned()),
            pose: ContainerPoseHandle::new(identifier.clone()),
            identifier,
            profile: manifest.try_into()?,
            t,
            processes: HashMap::new(),
            parent_container: None,
        })
    }

    pub fn new_with_parent(
        manifest: ContainerManifest,
        t: Box<S>,
        parent_container: ContainerPtr,
    ) -> JuizResult<Self> {
        //let type_name = obj_get_str(&manifest, "type_name")?;
        //let object_name = obj_get_str(&manifest, "name")?;
        let identifier = manifest.identifier()?;
        Ok(ContainerImpl {
            pose: ContainerPoseHandle::new(identifier.clone()),
            identifier,
            profile: manifest.try_into()?,
            t,
            processes: HashMap::new(),
            parent_container: Some(parent_container),
        })
    }

    /// profile を書き換える。CompositeContainer がスロットを profile に載せるのに使う (issue #168)
    pub fn profile_mut(&mut self) -> &mut ContainerProfile {
        &mut self.profile
    }

    /// 基準 Container `reference` (ID か名前) から見たこの Container の ContainerPose を読む
    /// (docs/spec/transformations.md §7.1)。戻り値の `header.frame_id` は基準の名前。
    ///
    /// 読み出しなので `&self` (§14)。計算は System の Transformations 登録所が行い、
    /// 登録所につながっていない (System に登録されていない) Container ではエラーになる。
    ///
    /// ```ignore
    /// #[juiz_component_container_process(container_type = "mobile_base")]
    /// fn mobile_base_update(c: &mut ContainerImpl<MobileBase>) -> JuizResult<Capsule> {
    ///     let p = c.pose("map")?;
    ///     Ok(p.into())
    /// }
    /// ```
    pub fn pose(&self, reference: &str) -> JuizResult<PoseStamped> {
        self.pose.pose(reference)
    }

    /// この Container を、基準 Container `reference` から見て `pose` の位置姿勢にする
    /// (docs/spec/transformations.md §7.2)。書き込みなので `&mut self` (§14)。
    ///
    /// 実際に書き換わるのは親 → この Container の Transform。どの Anchor の木にも入っていない
    /// Container ではエラーになり、Transform は自動では作られない。戻り値は書いた後の `pose(reference)`。
    pub fn set_pose(&mut self, reference: &str, pose: Pose) -> JuizResult<PoseStamped> {
        self.pose.set_pose(reference, &pose)
    }

    /// ContainerPose の読み出しだけの口 (§14)
    pub fn pose_reader(&self) -> ContainerPoseReader {
        self.pose.reader()
    }

    /// この Container の PoseState (Anchor の ID → 値)
    pub fn pose_state(&self) -> PoseStatePtr {
        self.pose.state()
    }
}

impl<S: 'static> Deref for ContainerImpl<S> {
    type Target = Box<S>;

    fn deref(&self) -> &Self::Target {
        &self.t
    }
}

impl<S: 'static> DerefMut for ContainerImpl<S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.t
    }
}

// impl<S: 'static> JuizObjectCoreHolder for ContainerImpl<S> {
//     fn core(&self) -> &ObjectCore {
//         &self.core
//     }
// }

// impl<S: 'static> JuizObject for ContainerImpl<S> {
//     fn profile_full(&self) -> JuizResult<Value> {
//         log::trace!("ContainerImpl({})::profile_full() called", self.identifier());
//         let ids = self.processes().iter().map(|p| -> JuizResult<Identifier> { Ok(p.identifier().to_string()) }).collect::<JuizResult<Vec<Identifier>>>()?;
//         obj_merge(self.core.profile_full()?, &jvalue!({
//             "processes": ids}))
//     }
// }

impl<S: 'static> Container for ContainerImpl<S> {
    fn identifier(&self) -> ContainerIdentifier {
        self.identifier.clone()
    }

    fn profile(&self) -> JuizResult<ContainerProfile> {
        Ok(self.profile.clone())
    }

    fn process(&self, name_or_id: &String) -> Option<ProcessPtr> {
        log::trace!(
            "ContainerImpl({}):process({}) called",
            self.identifier(),
            name_or_id
        );
        for (k, p) in self.processes.iter() {
            if k == name_or_id {
                return Some(p.clone());
            }
            match IdentifierStruct::try_from(name_or_id.clone()) {
                Ok(s) => {
                    if &s.object_name == name_or_id {
                        return Some(p.clone());
                    }
                }
                Err(_) => {}
            }
        }
        None
    }

    fn processes(&self) -> Vec<ProcessPtr> {
        self.processes
            .iter()
            .map(|(_k, p)| p.clone())
            .collect::<Vec<ProcessPtr>>()
    }

    fn register_process(&mut self, p: ProcessPtr) -> JuizResult<ProcessPtr> {
        let id = p.identifier().clone();
        self.processes.insert(id.to_string(), p.clone());
        Ok(p)
    }

    fn purge_process(&mut self, name_or_id: &String) -> JuizResult<()> {
        log::trace!(
            "ContainerImpl({})::purge_process({}) called",
            self.identifier(),
            name_or_id
        );

        match self.process(name_or_id) {
            Some(p) => {
                //let _ = p.write().unwrap().purge()?;
                let _res = self.processes.remove(&p.identifier().to_string());
                //log::trace!("ContainerImpl::purge_process({}) result: {:?}", name_or_id, res.is_some());
                Ok(())
            }
            None => {
                log::error!("purge_process({}) failed. Process is None.", name_or_id);
                Err(anyhow::Error::from(JuizError::ObjectCanNotFoundByIdError {
                    id: name_or_id.clone(),
                }))
            }
        }
    }

    fn clear(&mut self) -> JuizResult<()> {
        log::trace!("ContainerImpl({})::clear() called", self.identifier());
        for (_k, p) in self.processes.iter() {
            p.lock_mut()?.purge()?;
        }
        self.processes.clear();
        Ok(())
    }

    fn pose_handle(&self) -> ContainerPoseHandle {
        self.pose.clone()
    }
}

impl<S: 'static> Display for ContainerImpl<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ContainerImpl(identifier={})", self.identifier())
    }
}

impl<S: 'static> Drop for ContainerImpl<S> {
    fn drop(&mut self) {
        let id = &self.identifier;
        log::info!("ContainerImpl({})::drop() called", id);
        self.processes.clear();
        log::trace!("ContainerImpl({})::drop() exit", id);
    }
}
