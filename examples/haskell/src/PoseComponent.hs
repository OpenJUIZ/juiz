{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE TemplateHaskell #-}
-- | ContainerProcess から ContainerPose を読み書きする Component (pose_component_hs、issue #372)。
-- C++ の例 (examples/cpp/component/cpp_pose_component) と同じ構成。
-- juiz のテスト juiz_core/tests/integration/haskell_transformations_test.rs が使う。
module PoseComponent () where

import Control.Exception (try)

import Juiz

component :: ComponentManifest
component = componentManifest "pose_component_hs"
  & description "Haskell component that reads and writes ContainerPose."

-- | 利用者の状態は持たない部品。位置姿勢は juiz が Container ごとに持つ (利用者の状態とは別)
partManifest :: ContainerManifest
partManifest = containerManifest "hs_part"
  & description "A part with ContainerPose."

createPart :: IO (IORef Int64)
createPart = newIORef 0

-- | 基準 reference から見た自分の ContainerPose (juiz_std.geometry.PoseStamped の辞書)
partPoseManifest :: ProcessManifest
partPoseManifest = containerProcessManifest "hs_part_pose" "hs_part"
  & addStringArg "reference" "reference container" "map"

partPose :: IORef Int64 -> Text -> IO Value
partPose _ reference = poseStampedToValue reference <$> containerPose reference

-- | 自分を基準 reference から見て (x, y, z)・(qx, qy, qz, qw) にする
partSetPoseManifest :: ProcessManifest
partSetPoseManifest = containerProcessManifest "hs_part_set_pose" "hs_part"
  & addStringArg "reference" "reference container" "map"
  & addFloatArg "x" "" 0 & addFloatArg "y" "" 0 & addFloatArg "z" "" 0
  & addFloatArg "qx" "" 0 & addFloatArg "qy" "" 0 & addFloatArg "qz" "" 0 & addFloatArg "qw" "" 1

partSetPose :: IORef Int64 -> Text -> Double -> Double -> Double -> Double -> Double -> Double -> Double -> IO Value
partSetPose _ reference x y z qx qy qz qw =
  poseStampedToValue reference <$> setContainerPose reference (ContainerPose (x, y, z) (qx, qy, qz, qw))

-- | 読み書きを試し、エラーの種類か "ok" を返す
partTryManifest :: ProcessManifest
partTryManifest = containerProcessManifest "hs_part_try" "hs_part"
  & addStringArg "op" "pose / set_pose / set_pose_zero" "pose"
  & addStringArg "reference" "reference container" "map"

partTry :: IORef Int64 -> Text -> Text -> IO Text
partTry _ op reference = do
  r <- try $ case op of
    "pose" -> () <$ containerPose reference
    "set_pose" -> () <$ setContainerPose reference (ContainerPose (1, 0, 0) (0, 0, 0, 1))
    "set_pose_zero" -> () <$ setContainerPose reference (ContainerPose (0, 0, 0) (0, 0, 0, 0))
    _ -> pure ()
  pure $ case r of
    Right () -> "ok"
    Left (JuizError code _)
      | code == juizTransformationsNotAttachedError -> "TransformationsNotAttachedError"
      | code == juizNoCommonAnchorError -> "NoCommonAnchorError"
      | code == juizPoseNotWritableError -> "PoseNotWritableError"
      | code == juizInvalidPoseValueError -> "InvalidPoseValueError"
      | otherwise -> "TransformationsError"

juizComponent 'component
  [ componentContainer 'createPart 'partManifest
  , componentContainerProcess 'partPose 'partPoseManifest
  , componentContainerProcess 'partSetPose 'partSetPoseManifest
  , componentContainerProcess 'partTry 'partTryManifest
  ]
