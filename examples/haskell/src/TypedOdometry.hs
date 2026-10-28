{-# LANGUAGE OverloadedStrings #-}
-- | juiz-idl を使わずに手で書いた型付きの型 (docs/spec/datatype.md §22)。
-- Avro のスキーマから型を作り、'JuizType' と 'FromCapsule' / 'ToCapsule' のインスタンスを書く
module TypedOdometry
  ( Odometry (..)
  ) where

import Juiz
import Juiz.Type (field, recordFromValue)

data Odometry = Odometry
  { odometryDistance :: Double
  , odometrySteps :: Int64
  }
  deriving (Eq, Show)

odometryType :: TypeInfo
odometryType = unsafeTypeInfo
  "{\"type\": \"record\", \"name\": \"Odometry\", \"namespace\": \"handwritten\", \"fields\": [\
  \{\"name\": \"distance\", \"type\": \"double\"}, {\"name\": \"steps\", \"type\": \"long\", \"default\": 0}]}"
{-# NOINLINE odometryType #-}

instance JuizType Odometry where
  juizType _ = odometryType

instance FromCapsule Odometry where
  fromValue v = recordFromValue "handwritten.Odometry" v $ \o ->
    Odometry <$> field o "distance" Nothing <*> field o "steps" (Just (JInt 0))
  fromCapsuleType _ = Just odometryType

instance ToCapsule Odometry where
  toValue o = object ["distance" .= odometryDistance o, "steps" .= odometrySteps o]
  toCapsuleType _ = Just odometryType
