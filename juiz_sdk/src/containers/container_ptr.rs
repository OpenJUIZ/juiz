use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::{
    geometry::{Pose, PoseStamped, Transform, TransformStamped},
    prelude::*,
    transformations::{ContainerPoseHandle, ContainerPoseReader, PoseStatePtr, TransformationsRegistry},
};
use anyhow::anyhow;

#[derive(Clone)]
pub struct ContainerPtr {
    identifier: ContainerIdentifier,
    ptr: Arc<RwLock<dyn Container>>,
    /// ContainerImpl と同じ ContainerPose の口。Container のロックを取らずに使える (§9.1)
    pose: ContainerPoseHandle,
}

impl std::fmt::Debug for ContainerPtr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContainerPtr")
            .field("identifier", &self.identifier)
            .finish()
    }
}

impl ContainerPtr {
    pub fn new(container: impl Container) -> Self {
        ContainerPtr {
            identifier: container.identifier().clone(),
            pose: container.pose_handle(),
            ptr: Arc::new(RwLock::new(container)),
        }
    }

    /// ContainerPose の口 (ContainerImpl と同じものを指す)。Container のロックは取らない
    pub fn pose_handle(&self) -> ContainerPoseHandle {
        self.pose.clone()
    }

    /// この Container の PoseState。Container のロックは取らない
    pub fn pose_state(&self) -> PoseStatePtr {
        self.pose.state()
    }

    /// ContainerPose の読み出しだけの口 (§14)
    pub fn pose_reader(&self) -> ContainerPoseReader {
        self.pose.reader()
    }

    /// System に登録したときに Transformations 登録所への口をつなぐ (§9.1)。Container のロックは取らない
    pub fn attach_transformations(
        &self,
        registry: std::sync::Weak<dyn TransformationsRegistry>,
    ) -> JuizResult<()> {
        self.pose.attach(registry)
    }

    /// Transformations 登録所から外す
    pub fn detach_transformations(&self) -> JuizResult<()> {
        self.pose.detach()
    }

    /// 基準 `reference` から見たこの Container の ContainerPose (§7.1)。Container のロックは取らない
    pub fn pose(&self, reference: &str) -> JuizResult<PoseStamped> {
        self.pose.pose(reference)
    }

    /// 別の Container `target` の、基準 `reference` から見た ContainerPose (§7.4)。Container のロックは取らない
    pub fn lookup(&self, target: &str, reference: &str) -> JuizResult<PoseStamped> {
        self.pose.lookup(target, reference)
    }

    /// この Container から Transform (`source` → `destination`) の値を書く (§7.5、#410)。Container のロックは取らない
    pub fn set_transform(
        &self,
        source: &str,
        destination: &str,
        value: &Transform,
    ) -> JuizResult<TransformStamped> {
        self.pose.set_transform(source, destination, value)
    }

    /// この Container を基準 `reference` から見て `pose` にする (§7.2)。Container のロックは取らない
    pub fn set_pose(&self, reference: &str, pose: &Pose) -> JuizResult<PoseStamped> {
        self.pose.set_pose(reference, pose)
    }

    pub fn identifier(&self) -> ContainerIdentifier {
        self.identifier.clone()
    }

    pub fn lock(&self) -> JuizResult<RwLockReadGuard<dyn Container>> {
        self.ptr.read().or_else(|_| {
            Err(anyhow!(JuizError::ObjectLockError {
                target: "ContainerPtr".to_owned()
            }))
        })
    }

    pub fn lock_mut(&self) -> JuizResult<RwLockWriteGuard<dyn Container>> {
        self.ptr.write().or_else(|_| {
            Err(anyhow!(JuizError::ObjectLockError {
                target: "ContainerPtr".to_owned()
            }))
        })
    }

    // pub fn downcast_and_then<T: 'static, R, F>(&self, func: F) -> JuizResult<R> where F: FnOnce(&ContainerImpl<T>)->R {
    //     match self.lock()?.downcast_ref::<ContainerImpl<T>>() {
    //         None => Err(anyhow::Error::from(JuizError::ContainerDowncastingError{identifier: self.identifier.clone()})),
    //         Some(container_impl) => {
    //             Ok(func(container_impl))
    //         }
    //     }
    // }

    // pub fn downcast_mut_and_then<T: 'static, R, F>(&self, func: F) -> JuizResult<R> where F: FnOnce(&mut ContainerImpl<T>)->R {
    //     match self.lock_mut()?.downcast_mut::<ContainerImpl<T>>() {
    //         None => Err(anyhow::Error::from(JuizError::ContainerDowncastingError{identifier: self.identifier.clone()})),
    //         Some(container_impl) => {
    //             Ok(func(container_impl))
    //         }
    //     }
    // }
}
