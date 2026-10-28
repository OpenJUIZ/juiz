{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE TemplateHaskell #-}
-- | 型付きの引数・戻り値のテスト用 Component (typed_component_hs、juiz_core/tests/integration/haskell_typed_test.rs、
-- issue #196、docs/spec/datatype.md §22)。
--
-- 型は juiz-idl -g haskell が juiz_idl/test_package/idl/geometry.jidl から作ったもの (build.sh が gen/ に作る) と、
-- 手で書いた TypedOdometry の Odometry を使う。C++ の cpp_typed_component と同じ構成。
module TypedComponent () where

import qualified Data.ByteString as BS
import qualified Data.Map.Strict as Map
import qualified Data.Text as T
import Juiz

import Robot.Common (Header (..))
import Robot.Geometry
import TypedOdometry

origin :: Pose2D
origin = Pose2D { pose2DPosition = Point2D 0 0, pose2DTheta = 0, pose2DFrame = "map", pose2DComment = Nothing }

component :: ComponentManifest
component = componentManifest "typed_component_hs"
  & description "Haskell typed component (test)."

-- 結果が型付きの型なら output_schema は書かなくても載る
poseMakeManifest :: ProcessManifest
poseMakeManifest = processManifest "hs_pose_make"
  & addFloatArg "x" "" 0

poseMake :: Double -> Pose2D
poseMake x = origin { pose2DPosition = Point2D x 0 }

poseMoveManifest :: ProcessManifest
poseMoveManifest = processManifest "hs_pose_move"
  & addTypedArg "pose" "" origin
  & addFloatArg "dx" "" 1
  & returns (Proxy :: Proxy Pose2D)

poseMove :: Pose2D -> Double -> Pose2D
poseMove pose dx = pose { pose2DPosition = (pose2DPosition pose) { point2DX = point2DX (pose2DPosition pose) + dx } }

poseXManifest :: ProcessManifest
poseXManifest = processManifest "hs_pose_x"
  & addTypedArgNoDefault (Proxy :: Proxy Pose2D) "pose" ""
  & addTypedArg "offset" "" origin

poseX :: Pose2D -> Pose2D -> Double
poseX pose offset = point2DX (pose2DPosition pose) + point2DX (pose2DPosition offset)

-- Pose2D の frame と comment (既定値のあるフィールド) を文字列で返す
poseFrameManifest :: ProcessManifest
poseFrameManifest = processManifest "hs_pose_frame"
  & addTypedArgNoDefault (Proxy :: Proxy Pose2D) "pose" ""

poseFrame :: Pose2D -> Text
poseFrame pose = pose2DFrame pose <> "/" <> maybe "none" id (pose2DComment pose)

-- いろいろな型のフィールドを Haskell の値にして、少し変えて返す (C++ の cpp_everything_echo と同じ変え方)
everythingEchoManifest :: ProcessManifest
everythingEchoManifest = processManifest "hs_everything_echo"
  & addTypedArgNoDefault (Proxy :: Proxy Everything) "value" ""

everythingEcho :: Everything -> Everything
everythingEcho e = e
  { everythingI = everythingI e + 1
  , everythingL = everythingL e + 1
  , everythingData = everythingData e <> BS.pack [255]
  , everythingList = everythingList e ++ [fromIntegral (length (everythingList e))]
  , everythingTable = Map.insert "b" 2.5 (everythingTable e)
  , everythingMaybe = Nothing
  , everythingMode = Just MotorModePosition
  , everythingHeader = (everythingHeader e) { headerFrameId = headerFrameId (everythingHeader e) <> "!" }
  , everythingType = everythingType e <> "?"
  }

-- 列挙の引数と戻り値 (辞書の記号の文字列で渡る)
modeNextManifest :: ProcessManifest
modeNextManifest = processManifest "hs_mode_next"
  & addTypedArg "mode" "" MotorModeIdle

modeNext :: MotorMode -> MotorMode
modeNext MotorModeIdle = MotorModeVelocity
modeNext MotorModeVelocity = MotorModePosition
modeNext MotorModePosition = MotorModeIdle

-- 手で書いた型 (TypedOdometry)
odometryStepManifest :: ProcessManifest
odometryStepManifest = processManifest "hs_odometry_step"
  & addTypedArg "odometry" "" (Odometry 0 0)

odometryStep :: Odometry -> Odometry
odometryStep o = Odometry (odometryDistance o + 1) (odometrySteps o + 1)

-- 型付きの引数を持つ Container と ContainerProcess。受け取った Pose2D を覚え、x を scale 倍して返す
holderManifest :: ContainerManifest
holderManifest = containerManifest "hs_pose_holder"
  & addTypedArg "initial" "" origin

createHolder :: Pose2D -> IO (IORef Pose2D)
createHolder = newIORef

holderScaleManifest :: ProcessManifest
holderScaleManifest = containerProcessManifest "hs_pose_holder_scale" "hs_pose_holder"
  & addTypedArgNoDefault (Proxy :: Proxy Pose2D) "pose" ""
  & addFloatArg "scale" "" 2

holderScale :: IORef Pose2D -> Pose2D -> Double -> IO Pose2D
holderScale ref pose scale = do
  writeIORef ref pose
  pure pose { pose2DPosition = (pose2DPosition pose) { point2DX = point2DX (pose2DPosition pose) * scale } }

holderLastManifest :: ProcessManifest
holderLastManifest = containerProcessManifest "hs_pose_holder_last" "hs_pose_holder"

holderLast :: IORef Pose2D -> IO Text
holderLast ref = T.pack . show . point2DX . pose2DPosition <$> readIORef ref

juizComponent 'component
  [ componentProcess 'poseMake 'poseMakeManifest
  , componentProcess 'poseMove 'poseMoveManifest
  , componentProcess 'poseX 'poseXManifest
  , componentProcess 'poseFrame 'poseFrameManifest
  , componentProcess 'everythingEcho 'everythingEchoManifest
  , componentProcess 'modeNext 'modeNextManifest
  , componentProcess 'odometryStep 'odometryStepManifest
  , componentContainer 'createHolder 'holderManifest
  , componentContainerProcess 'holderScale 'holderScaleManifest
  , componentContainerProcess 'holderLast 'holderLastManifest
  ]
