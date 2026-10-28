{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE ScopedTypeVariables #-}
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
--
-- Component ('componentManifest') は、1 つのライブラリにまとめた Process / Container / ContainerProcess の
-- マニフェストを束ねたもの。ふつうは 'Juiz.TH.juizComponent' が組み立てる。
module Juiz.Manifest
  ( ProcessManifest (..)
  , ContainerManifest (..)
  , ComponentManifest (..)
  , ArgumentManifest (..)
  , HasDescription (..)
  , HasArguments (..)
  , processManifest
  , containerManifest
  , containerProcessManifest
  , componentManifest
  , addProcess
  , addContainer
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
  , addTypedArg
  , addTypedArgValue
  , addTypedArgNoDefault
  , returns
  , manifestToValue
  , containerManifestToValue
  , componentManifestToValue
  ) where

import Data.Int (Int64)
import qualified Data.Map.Strict as Map
import Data.Proxy (Proxy (..))
import Data.Text (Text)

import Juiz.Capsule (ToCapsule (..), object, (.=))
import Juiz.Internal.TypeInfo
import Juiz.Type (JuizType (..))
import Juiz.Value

data ArgumentManifest = ArgumentManifest
  { argName :: !Text
  -- | juiz の型名 ("Bool", "Int", "Float", "String", "Array", "Object")
  , argTypeName :: !Text
  , argDescription :: !Text
  , argDefault :: !Value
  -- | 型付きの引数の型 ('addTypedArg' など)。manifest の schema になり、呼ばれたときは値をこの型で読む
  , argType :: !(Maybe TypeInfo)
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
  -- | 戻り値の型 ('returns'、または関数の結果の型)。manifest の output_schema になる
  , pmOutputType :: !(Maybe TypeInfo)
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

-- | 1 つのライブラリにまとめる Process / Container / ContainerProcess のマニフェスト。
-- ContainerProcess ('pmContainerType' のある 'ProcessManifest') は、juiz に渡すときに
-- 同じ型名の Container の processes に入れる
data ComponentManifest = ComponentManifest
  { compTypeName :: !Text
  , compDescription :: !Text
  , compLanguage :: !Text
  , compContainers :: ![ContainerManifest]
  -- | Process と ContainerProcess (足した順)
  , compProcesses :: ![ProcessManifest]
  }
  deriving (Eq, Show)

-- | 説明を持つマニフェスト
class HasDescription m where
  setDescription :: Text -> m -> m

-- | 説明・シンボル名・引数を持つマニフェスト ('ProcessManifest' と 'ContainerManifest')
class HasDescription m => HasArguments m where
  setFactory :: Text -> m -> m
  appendArgument :: ArgumentManifest -> m -> m
  manifestArguments :: m -> [ArgumentManifest]

instance HasDescription ProcessManifest where
  setDescription d m = m { pmDescription = d }

instance HasArguments ProcessManifest where
  setFactory f m = m { pmFactory = f }
  appendArgument a m = m { pmArguments = pmArguments m ++ [a] }
  manifestArguments = pmArguments

instance HasDescription ContainerManifest where
  setDescription d m = m { cmDescription = d }

instance HasDescription ComponentManifest where
  setDescription d m = m { compDescription = d }

instance HasArguments ContainerManifest where
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
  , pmOutputType = Nothing
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

-- | Component のマニフェストを作る。名前が設定ファイルの components に書く名前とライブラリの名前になる
componentManifest :: Text -> ComponentManifest
componentManifest typeName = ComponentManifest
  { compTypeName = typeName
  , compDescription = ""
  , compLanguage = "haskell"
  , compContainers = []
  , compProcesses = []
  }

-- | Component に Process か ContainerProcess を足す。'pmFactory' は Component の中のシンボル名にしておく
addProcess :: ProcessManifest -> ComponentManifest -> ComponentManifest
addProcess p c = c { compProcesses = compProcesses c ++ [p] }

-- | Component に Container を足す。'cmFactory' は Component の中のシンボル名にしておく
addContainer :: ContainerManifest -> ComponentManifest -> ComponentManifest
addContainer m c = c { compContainers = compContainers c ++ [m] }

description :: HasDescription m => Text -> m -> m
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
addArg typeName name desc def = appendArgument (ArgumentManifest name typeName desc def Nothing)

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

-- | 型付きの引数を足す (型は juiz-idl -g haskell の生成した型など、docs/spec/datatype.md §22)。
-- 名前 → 説明 → 既定値 (その型の値)。
--
-- > addTypedArg "target" "goal pose" defaultPose2D
addTypedArg :: forall a m. (JuizType a, HasArguments m) => Text -> Text -> a -> m -> m
addTypedArg name desc def = addTypedArgValue (Proxy :: Proxy a) name desc (toValue def)

-- | 型付きの引数を足す。既定値は辞書の表し方 (仕様 §4) で書く。'JNull' は既定値なし
addTypedArgValue :: (JuizType a, HasArguments m) => Proxy a -> Text -> Text -> Value -> m -> m
addTypedArgValue p name desc def =
  let t = juizType p
  in appendArgument (ArgumentManifest name (typeInfoTypeName t) desc def (Just t))

-- | 既定値のない型付きの引数を足す (呼ぶときに必ず渡す)
--
-- > addTypedArgNoDefault (Proxy :: Proxy Pose2D) "target" "goal pose"
addTypedArgNoDefault :: (JuizType a, HasArguments m) => Proxy a -> Text -> Text -> m -> m
addTypedArgNoDefault p name desc = addTypedArgValue p name desc JNull

-- | 戻り値の型 (output_schema)。関数の結果が型付きの型なら書かなくても載る。書いたときは関数の結果の型と同じにする
returns :: JuizType a => Proxy a -> ProcessManifest -> ProcessManifest
returns p m = m { pmOutputType = Just (juizType p) }

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
  ++ maybe [] outputSchema (pmOutputType m)
  where
    -- 古いクライアント向けに、type_name だけの outputs も載せる (仕様 §6)
    outputSchema t =
      [ "output_schema" .= typeInfoSchema t
      , "outputs" .= object
          [ "enum_type" .= ("Primitive" :: Text)
          , "type_name" .= typeInfoTypeName t
          , "default_value" .= JNull
          ]
      ]

-- | juiz に渡す形 (C++ の ContainerManifest::into_value と同じ項目)
containerManifestToValue :: ContainerManifest -> Value
containerManifestToValue = containerWithProcessesToValue []

containerWithProcessesToValue :: [ProcessManifest] -> ContainerManifest -> Value
containerWithProcessesToValue ps m = object
  [ "type_name" .= cmTypeName m
  , "language" .= cmLanguage m
  , "description" .= cmDescription m
  , "factory" .= cmFactory m
  , "arguments" .= map argumentToValue (cmArguments m)
  , "processes" .= map manifestToValue ps
  ]

-- | juiz に渡す形 (C++ の ComponentManifest::into_value と同じ項目)。
-- ContainerProcess の Container が Component に無い、または型名が重なっていれば Left (説明)
componentManifestToValue :: ComponentManifest -> Either Text Value
componentManifestToValue c
  | (n : _) <- duplicates (map cmTypeName (compContainers c) ++ map pmTypeName (compProcesses c)) =
      Left ("the type name \"" <> n <> "\" is used twice in the component")
  | (p : _) <- filter (not . hasContainer) containerProcesses =
      Left ("the container process \"" <> pmTypeName p <> "\" belongs to the container \""
        <> maybe "" id (pmContainerType p) <> "\", which is not in the component")
  | otherwise = Right $ object
      [ "type_name" .= compTypeName c
      , "language" .= compLanguage c
      , "description" .= compDescription c
      , "containers" .= map containerValue (compContainers c)
      , "processes" .= map manifestToValue [p | p <- compProcesses c, pmContainerType p == Nothing]
      ]
  where
    containerProcesses = [p | p <- compProcesses c, pmContainerType p /= Nothing]
    hasContainer p = pmContainerType p `elem` map (Just . cmTypeName) (compContainers c)
    containerValue m =
      containerWithProcessesToValue [p | p <- containerProcesses, pmContainerType p == Just (cmTypeName m)] m
    duplicates xs = [x | (i, x) <- zip [0 :: Int ..] xs, x `elem` take i xs]

argumentToValue :: ArgumentManifest -> Value
argumentToValue a = object $
  [ "name" .= argName a
  , "type_name" .= argTypeName a
  , "description" .= argDescription a
  , "default" .= argDefault a
  ]
  ++ maybe [] (\t -> ["schema" .= typeInfoSchema t]) (argType a)
