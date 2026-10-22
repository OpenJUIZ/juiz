{-# LANGUAGE OverloadedStrings #-}
-- | Process / Container のマニフェストの組み立て。
--
-- > manifest :: ProcessManifest
-- > manifest = processManifest "increment_process_hs"
-- >   & description "Adds 1 to arg1."
-- >   & addIntArg "arg1" "value to be incremented" 1
--
-- 引数を足した順が、Process の関数の引数の順になる。
-- Container ('containerManifest') では、Container を作る関数の引数の順になる。
-- ContainerProcess ('containerProcessManifest') では、第 1 引数 (Container の状態) の後に続く引数の順になる。
module Juiz.Manifest
  ( ProcessManifest (..)
  , ContainerManifest (..)
  , ArgumentManifest (..)
  , HasArguments (..)
  , processManifest
  , containerManifest
  , containerProcessManifest
  , description
  , factory
  , useMemo
  , containerType
  , addArg
  , addBoolArg
  , addIntArg
  , addFloatArg
  , addStringArg
  , addArrayArg
  , addObjectArg
  , manifestToValue
  , containerManifestToValue
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
  -- | ContainerProcess のとき、結びつける Container の型名
  , pmContainerType :: !(Maybe Text)
  }
  deriving (Eq, Show)

data ContainerManifest = ContainerManifest
  { cmTypeName :: !Text
  , cmDescription :: !Text
  , cmLanguage :: !Text
  -- | 関数を探すときのシンボル名。単体の Container は "container_factory"
  , cmFactory :: !Text
  -- | Container を作る関数の引数 (設定ファイルの containers に書いた値)
  , cmArguments :: ![ArgumentManifest]
  }
  deriving (Eq, Show)

-- | 説明・シンボル名・引数を持つマニフェスト ('ProcessManifest' と 'ContainerManifest')
class HasArguments m where
  setDescription :: Text -> m -> m
  setFactory :: Text -> m -> m
  appendArgument :: ArgumentManifest -> m -> m
  manifestArguments :: m -> [ArgumentManifest]

instance HasArguments ProcessManifest where
  setDescription d m = m { pmDescription = d }
  setFactory f m = m { pmFactory = f }
  appendArgument a m = m { pmArguments = pmArguments m ++ [a] }
  manifestArguments = pmArguments

instance HasArguments ContainerManifest where
  setDescription d m = m { cmDescription = d }
  setFactory f m = m { cmFactory = f }
  appendArgument a m = m { cmArguments = cmArguments m ++ [a] }
  manifestArguments = cmArguments

-- | 型名 (設定ファイルの type_name) を指定してマニフェストを作る
processManifest :: Text -> ProcessManifest
processManifest typeName = ProcessManifest
  { pmTypeName = typeName
  , pmDescription = ""
  , pmLanguage = "haskell"
  , pmFactory = "process_factory"
  , pmArguments = []
  , pmUseMemo = False
  , pmContainerType = Nothing
  }

-- | 型名 (設定ファイルの type_name) を指定して Container のマニフェストを作る
containerManifest :: Text -> ContainerManifest
containerManifest typeName = ContainerManifest
  { cmTypeName = typeName
  , cmDescription = ""
  , cmLanguage = "haskell"
  , cmFactory = "container_factory"
  , cmArguments = []
  }

-- | ContainerProcess のマニフェストを作る。型名 → 結びつける Container の型名
containerProcessManifest :: Text -> Text -> ProcessManifest
containerProcessManifest typeName ctype = (processManifest typeName)
  { pmFactory = "container_process_factory"
  , pmContainerType = Just ctype
  }

description :: HasArguments m => Text -> m -> m
description = setDescription

factory :: HasArguments m => Text -> m -> m
factory = setFactory

-- | 結びつける Container の型名 (juiz の container_type)
containerType :: Text -> ProcessManifest -> ProcessManifest
containerType c m = m { pmContainerType = Just c }

-- | 同じ引数で呼ばれたときに前の結果を使い回すか (juiz の use_memo)
useMemo :: Bool -> ProcessManifest -> ProcessManifest
useMemo b m = m { pmUseMemo = b }

-- | 引数を足す。型名 → 名前 → 説明 → 既定値
addArg :: HasArguments m => Text -> Text -> Text -> Value -> m -> m
addArg typeName name desc def = appendArgument (ArgumentManifest name typeName desc def)

addBoolArg :: HasArguments m => Text -> Text -> Bool -> m -> m
addBoolArg name desc def = addArg "Bool" name desc (JBool def)

addIntArg :: HasArguments m => Text -> Text -> Int64 -> m -> m
addIntArg name desc def = addArg "Int" name desc (JInt def)

addFloatArg :: HasArguments m => Text -> Text -> Double -> m -> m
addFloatArg name desc def = addArg "Float" name desc (JFloat def)

addStringArg :: HasArguments m => Text -> Text -> Text -> m -> m
addStringArg name desc def = addArg "String" name desc (JString def)

addArrayArg :: HasArguments m => Text -> Text -> [Value] -> m -> m
addArrayArg name desc def = addArg "Array" name desc (JArray def)

addObjectArg :: HasArguments m => Text -> Text -> [(Text, Value)] -> m -> m
addObjectArg name desc def = addArg "Object" name desc (JObject (Map.fromList def))

-- | juiz に渡す形 (C++ の ProcessManifest::into_value と同じ項目)
manifestToValue :: ProcessManifest -> Value
manifestToValue m = object $
  [ "type_name" .= pmTypeName m
  , "language" .= pmLanguage m
  , "description" .= pmDescription m
  , "factory" .= pmFactory m
  , "use_memo" .= pmUseMemo m
  , "arguments" .= map argumentToValue (pmArguments m)
  ]
  ++ maybe [] (\c -> ["container_type" .= c]) (pmContainerType m)

-- | juiz に渡す形 (C++ の ContainerManifest::into_value と同じ項目)
containerManifestToValue :: ContainerManifest -> Value
containerManifestToValue m = object
  [ "type_name" .= cmTypeName m
  , "language" .= cmLanguage m
  , "description" .= cmDescription m
  , "factory" .= cmFactory m
  , "arguments" .= map argumentToValue (cmArguments m)
  , "processes" .= ([] :: [Value])
  ]

argumentToValue :: ArgumentManifest -> Value
argumentToValue a = object
  [ "name" .= argName a
  , "type_name" .= argTypeName a
  , "description" .= argDescription a
  , "default" .= argDefault a
  ]
