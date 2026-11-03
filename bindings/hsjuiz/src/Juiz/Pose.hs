{-# LANGUAGE OverloadedStrings #-}
-- | ContainerProcess の中から、呼び出し元 Container の位置姿勢 (ContainerPose) を読み書きする
-- (docs/spec/transformations.md §12.3)。
--
-- > update :: IORef Int64 -> Double -> IO Value
-- > update _ dx = do
-- >   (p, _) <- containerPose "map"                       -- map から見たこの Container の位置姿勢
-- >   let (x, y, z) = cpPosition p
-- >   (q, _) <- setContainerPose "map" p { cpPosition = (x + dx, y, z) }   -- 親 → この Container の Transform を書き換える
-- >   pure (poseToValue q)
--
-- ContainerProcess の関数は Container の状態だけを受け取り、Container そのものは受け取らないので、
-- 読み書きの相手は「今呼ばれている ContainerProcess の Container」になる。juiz が呼んだスレッドの上
-- (関数の中) でだけ使える。'Control.Concurrent.forkIO' した先や、関数から戻った後では
-- 'juizTransformationsNotAttachedError' になる。四元数の並びは (x, y, z, w)。
module Juiz.Pose
  ( ContainerPose (..)
  , PoseTime (..)
  , identityPose
  , containerPose
  , setContainerPose
  , poseToValue
  , poseStampedToValue
    -- * 内部 (ContainerProcess のエントリポイントが使う)
  , withPoseCall
  ) where

import Control.Exception (finally)
import Control.Monad (when)
import Data.Int (Int32, Int64)
import qualified Data.Map.Strict as Map
import Data.Text (Text)
import qualified Data.Text as T
import qualified Data.ByteString as BS
import qualified Data.Text.Encoding as TE
import Foreign.C.String (CString, peekCString)
import Foreign.Marshal.Alloc (allocaBytes)
import Foreign.Ptr (Ptr, nullPtr)
import Foreign.Storable (peekByteOff, pokeByteOff)

import Juiz.Error
import Juiz.Internal.FFI (CCapsuleMap)
import Juiz.Value

-- | 位置姿勢 (@juiz_std.geometry.Pose@)。位置 (x, y, z) [m] と四元数 (x, y, z, w)
data ContainerPose = ContainerPose
  { cpPosition :: !(Double, Double, Double)
  , cpOrientation :: !(Double, Double, Double, Double)
  }
  deriving (Eq, Show)

-- | 計算・書き込みの時刻 (@juiz_std.common.Time@)
data PoseTime = PoseTime
  { ptSec :: !Int64
  , ptNanosec :: !Int32
  }
  deriving (Eq, Show)

-- | 原点・回転なし
identityPose :: ContainerPose
identityPose = ContainerPose (0, 0, 0) (0, 0, 0, 1)

foreign import ccall unsafe "hsjuiz_swap_pose_call"
  c_hsjuiz_swap_pose_call :: Ptr CCapsuleMap -> IO (Ptr CCapsuleMap)

-- 書き込みは Pose 通知で Haskell の Process を呼ぶことがあるので safe で呼ぶ
foreign import ccall safe "juiz_container_pose_get"
  c_juiz_container_pose_get :: Ptr CCapsuleMap -> CString -> Ptr () -> IO Int64
foreign import ccall safe "juiz_container_pose_set"
  c_juiz_container_pose_set :: Ptr CCapsuleMap -> CString -> Ptr () -> Ptr () -> IO Int64
foreign import ccall unsafe "juiz_container_pose_last_error"
  c_juiz_container_pose_last_error :: IO CString

-- | ContainerProcess の関数を呼ぶ間、その引数 (呼び出し元 Container の口が載っている) を覚えておく。
-- 入れ子 (Pose 通知で同じスレッドから別の ContainerProcess が呼ばれる) でも元に戻す
withPoseCall :: Ptr CCapsuleMap -> IO a -> IO a
withPoseCall cm act = do
  old <- c_hsjuiz_swap_pose_call cm
  act `finally` c_hsjuiz_swap_pose_call old

currentPoseCall :: IO (Ptr CCapsuleMap)
currentPoseCall = do
  cm <- c_hsjuiz_swap_pose_call nullPtr
  _ <- c_hsjuiz_swap_pose_call cm
  when (cm == nullPtr) $
    throwJuiz juizTransformationsNotAttachedError
      "containerPose / setContainerPose can be used only inside a container process (on the thread that juiz called it)"
  pure cm

-- juiz_pose_stamped の並び: position (0, 8, 16)、orientation (24, 32, 40, 48)、stamp.sec 56、stamp.nanosec 64
pokePose :: Ptr () -> ContainerPose -> IO ()
pokePose p (ContainerPose (x, y, z) (qx, qy, qz, qw)) =
  mapM_ (\(o, v) -> pokeByteOff p o v) (zip [0, 8 .. 48] [x, y, z, qx, qy, qz, qw])

peekStamped :: Ptr () -> IO (ContainerPose, PoseTime)
peekStamped p = do
  [x, y, z, qx, qy, qz, qw] <- mapM (peekByteOff p) [0, 8 .. 48]
  sec <- peekByteOff p 56
  nsec <- peekByteOff p 64
  pure (ContainerPose (x, y, z) (qx, qy, qz, qw), PoseTime sec nsec)

check :: Int64 -> IO ()
check 0 = pure ()
check code = do
  msg <- c_juiz_container_pose_last_error >>= peekCString
  throwJuiz code (if null msg then "ContainerPose error (" <> T.pack (show code) <> ")" else T.pack msg)

-- | 基準 reference (Container の名前か ID) から見た、今呼ばれている ContainerProcess の Container の位置姿勢 (§7.1)
containerPose :: Text -> IO (ContainerPose, PoseTime)
containerPose reference = do
  cm <- currentPoseCall
  allocaBytes 72 $ \out ->
    withUtf8 reference $ \ref -> do
      c_juiz_container_pose_get cm ref out >>= check
      peekStamped out

-- | 今呼ばれている ContainerProcess の Container を、基準 reference から見て pose の位置姿勢にする (§7.2)。
-- 書き換わるのは親 → この Container の Transform。書いた後の値を返す
setContainerPose :: Text -> ContainerPose -> IO (ContainerPose, PoseTime)
setContainerPose reference pose = do
  cm <- currentPoseCall
  allocaBytes 56 $ \inp ->
    allocaBytes 72 $ \out ->
      withUtf8 reference $ \ref -> do
        pokePose inp pose
        c_juiz_container_pose_set cm ref inp out >>= check
        peekStamped out

-- | @juiz_std.geometry.Pose@ の辞書にする
poseToValue :: ContainerPose -> Value
poseToValue (ContainerPose (x, y, z) (qx, qy, qz, qw)) =
  JObject $ Map.fromList
    [ ("position", JObject $ Map.fromList [("x", JFloat x), ("y", JFloat y), ("z", JFloat z)])
    , ("orientation", JObject $ Map.fromList [("x", JFloat qx), ("y", JFloat qy), ("z", JFloat qz), ("w", JFloat qw)])
    ]

-- | @juiz_std.geometry.PoseStamped@ の辞書にする (@header.frame_id@ は reference)
poseStampedToValue :: Text -> (ContainerPose, PoseTime) -> Value
poseStampedToValue reference (pose, PoseTime sec nsec) =
  JObject $ Map.fromList
    [ ( "header"
      , JObject $ Map.fromList
          [ ("stamp", JObject $ Map.fromList [("sec", JInt sec), ("nanosec", JInt (fromIntegral nsec))])
          , ("frame_id", JString reference)
          ]
      )
    , ("pose", poseToValue pose)
    ]

withUtf8 :: Text -> (CString -> IO a) -> IO a
withUtf8 t = BS.useAsCString (TE.encodeUtf8 t)
