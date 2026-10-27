{-# LANGUAGE OverloadedStrings #-}
-- | 型付きの引数・戻り値の型 ('TypeInfo') のデータ。作るのは "Juiz.Type" の 'Juiz.Type.newTypeInfo'。
--
-- "Juiz.Capsule" の 'Juiz.Capsule.FromCapsule' / 'Juiz.Capsule.ToCapsule' がこの型を参照するので、
-- 循環しないようにデータだけをここに置く。
module Juiz.Internal.TypeInfo
  ( TypeInfo (..)
  , typeInfoName
  , typeInfoFingerprint
  ) where

import qualified Data.Map.Strict as Map
import Data.Text (Text)
import Foreign.Ptr (Ptr)

import Juiz.Internal.FFI (CJuizType)
import Juiz.Value

-- | juiz のデータ型 (docs/spec/datatype.md)。libjuiz_sdk の juiz_type と、それを juiz_type_describe で
-- 読んだ型参照・manifest の type_name を持つ。指紋は libjuiz_sdk (Rust の juiz_types) が計算するので、
-- .avsc・Rust・Python・C++ と一致する
data TypeInfo = TypeInfo
  { typeInfoHandle :: !(Ptr CJuizType)
  -- | 型参照 @{name, fingerprint, avro}@ (manifest の schema / output_schema に載せるもの)
  , typeInfoSchema :: !Value
  -- | manifest の type_name ("Object"・"String" など、仕様 §6 の表)
  , typeInfoTypeName :: !Text
  }

-- | 型の完全名 (@robot.geometry.Pose2D@ など)
typeInfoName :: TypeInfo -> Text
typeInfoName = schemaText "name"

-- | 型の指紋 (@sha256:…@)
typeInfoFingerprint :: TypeInfo -> Text
typeInfoFingerprint = schemaText "fingerprint"

schemaText :: Text -> TypeInfo -> Text
schemaText key t = case typeInfoSchema t of
  JObject m | Just (JString s) <- Map.lookup key m -> s
  _ -> ""

-- | 名前と指紋が同じなら同じ型
instance Eq TypeInfo where
  a == b = typeInfoName a == typeInfoName b && typeInfoFingerprint a == typeInfoFingerprint b

instance Show TypeInfo where
  show t = "TypeInfo " ++ show (typeInfoName t) ++ " " ++ show (typeInfoFingerprint t)
