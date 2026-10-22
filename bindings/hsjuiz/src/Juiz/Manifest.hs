{-# LANGUAGE OverloadedStrings #-}
-- | Process のマニフェストの組み立て。
--
-- > manifest :: ProcessManifest
-- > manifest = processManifest "increment_process_hs"
-- >   & description "Adds 1 to arg1."
-- >   & addIntArg "arg1" "value to be incremented" 1
--
-- 引数を足した順が、Process の関数の引数の順になる。
module Juiz.Manifest
  ( ProcessManifest (..)
  , ArgumentManifest (..)
  , processManifest
  , description
  , factory
  , useMemo
  , addArg
  , addBoolArg
  , addIntArg
  , addFloatArg
  , addStringArg
  , addArrayArg
  , addObjectArg
  , manifestToValue
  ) where

import Data.Int (Int64)
import qualified Data.Map.Strict as Map
import Data.Text (Text)

import Juiz.Capsule (object, (.=))
import Juiz.Value

data ArgumentManifest = ArgumentManifest
  { argName :: !Text
  -- | juiz の型名 ("Bool", "Int", "Float", "String", "Array", "Object")
  , argTypeName :: !Text
  , argDescription :: !Text
  , argDefault :: !Value
  }
  deriving (Eq, Show)

data ProcessManifest = ProcessManifest
  { pmTypeName :: !Text
  , pmDescription :: !Text
  , pmLanguage :: !Text
  -- | 関数を探すときのシンボル名。単体の Process は "process_factory"
  , pmFactory :: !Text
  , pmArguments :: ![ArgumentManifest]
  , pmUseMemo :: !Bool
  }
  deriving (Eq, Show)

-- | 型名 (設定ファイルの type_name) を指定してマニフェストを作る
processManifest :: Text -> ProcessManifest
processManifest typeName = ProcessManifest
  { pmTypeName = typeName
  , pmDescription = ""
  , pmLanguage = "haskell"
  , pmFactory = "process_factory"
  , pmArguments = []
  , pmUseMemo = False
  }

description :: Text -> ProcessManifest -> ProcessManifest
description d m = m { pmDescription = d }

factory :: Text -> ProcessManifest -> ProcessManifest
factory f m = m { pmFactory = f }

-- | 同じ引数で呼ばれたときに前の結果を使い回すか (juiz の use_memo)
useMemo :: Bool -> ProcessManifest -> ProcessManifest
useMemo b m = m { pmUseMemo = b }

-- | 引数を足す。型名 → 名前 → 説明 → 既定値
addArg :: Text -> Text -> Text -> Value -> ProcessManifest -> ProcessManifest
addArg typeName name desc def m =
  m { pmArguments = pmArguments m ++ [ArgumentManifest name typeName desc def] }

addBoolArg :: Text -> Text -> Bool -> ProcessManifest -> ProcessManifest
addBoolArg name desc def = addArg "Bool" name desc (JBool def)

addIntArg :: Text -> Text -> Int64 -> ProcessManifest -> ProcessManifest
addIntArg name desc def = addArg "Int" name desc (JInt def)

addFloatArg :: Text -> Text -> Double -> ProcessManifest -> ProcessManifest
addFloatArg name desc def = addArg "Float" name desc (JFloat def)

addStringArg :: Text -> Text -> Text -> ProcessManifest -> ProcessManifest
addStringArg name desc def = addArg "String" name desc (JString def)

addArrayArg :: Text -> Text -> [Value] -> ProcessManifest -> ProcessManifest
addArrayArg name desc def = addArg "Array" name desc (JArray def)

addObjectArg :: Text -> Text -> [(Text, Value)] -> ProcessManifest -> ProcessManifest
addObjectArg name desc def = addArg "Object" name desc (JObject (Map.fromList def))

-- | juiz に渡す形 (C++ の ProcessManifest::into_value と同じ項目)
manifestToValue :: ProcessManifest -> Value
manifestToValue m = object
  [ "type_name" .= pmTypeName m
  , "language" .= pmLanguage m
  , "description" .= pmDescription m
  , "factory" .= pmFactory m
  , "use_memo" .= pmUseMemo m
  , "arguments" .= map argumentToValue (pmArguments m)
  ]
  where
    argumentToValue a = object
      [ "name" .= argName a
      , "type_name" .= argTypeName a
      , "description" .= argDescription a
      , "default" .= argDefault a
      ]
