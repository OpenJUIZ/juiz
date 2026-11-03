"""Python の ContainerProcess から ContainerPose を読み書きする (issue #371)

仕様は docs/spec/transformations.md §12.2。

    from juiz import juiz_container_process, pose, set_pose
    from juiz_std.geometry import Pose, PoseStamped

    @juiz_container_process(container_type="mobile_base")
    def mobile_base_update(base, dx: float = 0.0) -> PoseStamped:
        p = pose(base, "map")                  # map から見た base の位置姿勢 (PoseStamped)
        p.pose.position.x += dx
        return set_pose(base, "map", p.pose)   # 親 → base の Transform を書き換える

第 1 引数 (利用者のオブジェクト) には、juiz が Container を作ったときに属性 ``__juiz_pose__``
(ホストの口) を付けている。``__slots__`` を持つクラスや組み込みの型 (dict など) には付けられないので、
Container の constructor は普通のクラスのオブジェクトを返すこと。

読み出し (pose) と書き込み (set_pose) は別の関数にしてある (将来の readonly ContainerProcess に備える、§14)。
"""

POSE_HANDLE_ATTRIBUTE = "__juiz_pose__"


class TransformationsError(RuntimeError):
    """ContainerPose の読み書きのエラーの基底 (仕様 §17)"""

    def __init__(self, kind: str, message: str):
        super().__init__(f"{kind}: {message}")
        self.kind = kind
        self.message = message


class NoCommonAnchorError(TransformationsError):
    """基準と Container の両方を木に含む Anchor がない"""


class PoseNotWritableError(TransformationsError):
    """根の Anchor・基準自身・基準が子孫の Container への書き込み"""


class InvalidPoseValueError(TransformationsError):
    """長さ 0 の四元数、NaN・無限大"""


class TransformationsNotAttachedError(TransformationsError):
    """System に登録されていない (Transformations につながっていない) Container の読み書き、
    または juiz が作った Container のオブジェクトでないものを渡したとき"""


_ERRORS = {
    c.__name__: c
    for c in (NoCommonAnchorError, PoseNotWritableError, InvalidPoseValueError, TransformationsNotAttachedError)
}


def _handle(container):
    h = getattr(container, POSE_HANDLE_ATTRIBUTE, None)
    if h is None:
        raise TransformationsNotAttachedError(
            "TransformationsNotAttachedError",
            "{!r} は juiz が作った Container のオブジェクトではありません (ContainerProcess の第 1 引数を渡してください。"
            "__slots__ を持つクラスや dict などの組み込みの型には口を付けられません)".format(type(container).__name__),
        )
    return h


def _call(f, *args):
    try:
        return f(*args)
    except RuntimeError as e:
        if len(e.args) == 2 and isinstance(e.args[0], str):
            kind, message = e.args
            raise _ERRORS.get(kind, TransformationsError)(kind, message) from None
        raise


def _reference(reference) -> str:
    # ContainerProxy など identifier() を持つものも受け付ける
    if hasattr(reference, "identifier") and callable(reference.identifier):
        return reference.identifier()
    if not isinstance(reference, str):
        raise TypeError("reference は Container の名前か ID (str) を渡してください: {!r}".format(reference))
    return reference


def _pose_value(value) -> dict:
    """set_pose に渡せる値 (Pose、PoseStamped、それぞれの dict) を Pose の dict にする"""
    if hasattr(value, "to_value") and callable(value.to_value):
        value = value.to_value()
    if not isinstance(value, dict):
        raise TypeError(
            "pose には juiz_std.geometry.Pose / PoseStamped か dict を渡してください: {!r}".format(type(value).__name__))
    if "pose" in value and "position" not in value and "orientation" not in value:
        # PoseStamped は pose だけを使う (基準は引数 reference。header は見ない)
        value = value["pose"]
    return value


def _pose_stamped(value):
    from juiz_std.geometry import PoseStamped
    return PoseStamped.from_value(value)


def pose(container, reference):
    """Container (ContainerProcess の第 1 引数) の、基準 reference (Container の名前か ID) から見た
    ContainerPose を juiz_std.geometry.PoseStamped で返す (§7.1)。header.frame_id は基準の名前"""
    h = _handle(container)
    return _pose_stamped(_call(h.pose, _reference(reference)))


def set_pose(container, reference, value):
    """Container を基準 reference から見て value (juiz_std.geometry.Pose / PoseStamped / dict) の
    位置姿勢にする (§7.2)。書き換わるのは親 → Container の Transform。PoseStamped は pose だけを使い、
    header は見ない (基準はいつも reference)。書いた後の pose(container, reference)
    を PoseStamped で返す"""
    h = _handle(container)
    ref = _reference(reference)
    return _pose_stamped(_call(h.set_pose, ref, _pose_value(value)))
