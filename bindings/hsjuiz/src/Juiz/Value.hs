-- | juiz の値 (Rust 側の serde_json::Value) を Haskell で表す型。
--
-- aeson の 'Data.Aeson.Value' に相当するが、juiz と同じく整数 ('JInt', 'JUInt') と小数 ('JFloat') を分けて持つ。
-- Haskell の値との変換は "Juiz.Capsule" の 'Juiz.Capsule.FromCapsule' / 'Juiz.Capsule.ToCapsule' で行う。
module Juiz.Value
  ( Value (..)
  ) where

import Data.Int (Int64)
import Data.Map.Strict (Map)
import Data.Text (Text)
import Data.Word (Word64)

data Value
  = JNull
  | JBool !Bool
  | JInt !Int64
  -- | Int64 に収まらない正の整数
  | JUInt !Word64
  | JFloat !Double
  | JString !Text
  | JArray ![Value]
  | JObject !(Map Text Value)
  deriving (Eq, Show)
