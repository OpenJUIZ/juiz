//! Python の ContainerProcess から ContainerPose を読み書きする口
//! (docs/spec/transformations.md §12.2、issue #371)
//!
//! Python の Container を作ったとき、ホストは利用者のオブジェクトに属性 `__juiz_pose__` として
//! [`PyContainerPoseHandle`] を付ける。pyjuiz の `juiz.pose(self, ref)` / `juiz.set_pose(self, ref, p)` は
//! この属性を通してホストの ContainerPoseHandle を呼ぶ。値は辞書 (`juiz_std.geometry` の表し方) で行き来し、
//! 型の値への変換は pyjuiz がする。

use crate::prelude::*;
use juiz_sdk::anyhow;
use juiz_sdk::geometry::Pose;
use juiz_sdk::transformations::ContainerPoseHandle;
use pyo3::{exceptions::PyRuntimeError, prelude::*};

use super::python_plugin::{pyany_to_value, value_to_pyany};

/// 利用者のオブジェクトに付ける属性の名前 (pyjuiz の juiz/transformations.py と同じ)
pub const POSE_HANDLE_ATTRIBUTE: &str = "__juiz_pose__";

/// ホストの ContainerPoseHandle を Python から呼ぶための包み。
///
/// 読み出し (`pose`) と書き込み (`set_pose`) は別のメソッドにする (§14)。
/// エラーは `RuntimeError(エラーの種類, メッセージ)` で返し、pyjuiz が種類ごとの例外にする。
#[pyclass(frozen, name = "ContainerPoseHandle", module = "juiz")]
pub struct PyContainerPoseHandle {
    handle: ContainerPoseHandle,
}

impl PyContainerPoseHandle {
    pub fn new(handle: ContainerPoseHandle) -> Self {
        Self { handle }
    }
}

#[pymethods]
impl PyContainerPoseHandle {
    /// この Container の ID
    fn identifier(&self) -> String {
        self.handle.identifier().to_string()
    }

    /// 基準 `reference` から見たこの Container の ContainerPose (PoseStamped の辞書)
    fn pose(&self, py: Python<'_>, reference: &str) -> PyResult<Py<PyAny>> {
        // 登録所のロックを待つ間は GIL を手放す (ほかのスレッドの Python を止めない)
        let r = py.detach(|| self.handle.pose(reference));
        let v: Value = r.map_err(to_py_error)?.into();
        Ok(value_to_pyany(py, &v))
    }

    /// この Container を基準 `reference` から見て `pose` (Pose の辞書) にする。書いた後の PoseStamped の辞書を返す
    fn set_pose(
        &self,
        py: Python<'_>,
        reference: &str,
        pose: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        let value = pyany_to_value(pose)?;
        let pose = Pose::try_from(&value).map_err(to_py_error)?;
        // Pose 通知の受け手 (Python の Process もありうる) は書き込んだスレッドで呼ばれるが、
        // その中で GIL を取り直すので、ここで手放してよい
        let r = py.detach(|| self.handle.set_pose(reference, &pose));
        let v: Value = r.map_err(to_py_error)?.into();
        Ok(value_to_pyany(py, &v))
    }

    fn __repr__(&self) -> String {
        format!("ContainerPoseHandle({})", self.handle.identifier())
    }
}

/// Container の利用者のオブジェクトに口を付ける。属性を付けられないオブジェクト
/// (`__slots__` を持つ、組み込みの型など) では WARN を出して付けない (そのときは `juiz.pose` がエラーになる)
pub fn attach_pose_handle(py: Python<'_>, pyobj: &Py<PyAny>, handle: ContainerPoseHandle) {
    let id = handle.identifier().to_string();
    let r = Py::new(py, PyContainerPoseHandle::new(handle))
        .and_then(|h| pyobj.bind(py).setattr(POSE_HANDLE_ATTRIBUTE, h));
    if let Err(e) = r {
        log::warn!(
            "Python の Container({id}) のオブジェクトに属性 {POSE_HANDLE_ATTRIBUTE} を付けられないので、juiz.pose / juiz.set_pose は使えません: {e}"
        );
    }
}

/// JuizError の変種の名前 (`NoCommonAnchorError` など)。JuizError でなければ None
fn error_kind(e: &anyhow::Error) -> Option<String> {
    e.chain().find_map(|c| {
        c.downcast_ref::<JuizError>().map(|j| {
            format!("{j:?}")
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect::<String>()
        })
    })
}

fn to_py_error(e: anyhow::Error) -> PyErr {
    let kind = error_kind(&e).unwrap_or_else(|| "JuizError".to_owned());
    PyRuntimeError::new_err((kind, format!("{e:#}")))
}
