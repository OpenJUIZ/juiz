{-# LANGUAGE FlexibleContexts #-}
{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE ScopedTypeVariables #-}
-- | Component (1 つのライブラリにまとめた Process / Container / ContainerProcess) の
-- component_manifest_entry_point を作る。
--
-- ふつうは "Juiz.TH" の 'Juiz.TH.juizComponent' が、各要素のエントリポイントと一緒に生成する。
-- Template Haskell を使わずに書く方法は README を参照。
module Juiz.Component
  ( ComponentPart
  , processPart
  , containerPart
  , containerProcessPart
  , componentManifestEntryPoint
  ) where

import Data.Proxy (Proxy (..))
import Data.Text (Text)

import Juiz.Capsule (ToCapsule, writeCapsulePtr)
import Juiz.Error
import Juiz.Internal.FFI
import Juiz.Manifest
import Juiz.Process
import Juiz.Value

-- | Component に入れる要素のマニフェスト (型を関数と比べて戻り値の型を補ったもの、合わなければ説明) と、関数の引数の数
data ComponentPart = ComponentPart
  { partAdd :: Either Text (ComponentManifest -> ComponentManifest)
  , partName :: Text
  , partArguments :: Int
  , partArity :: Int
  }

-- | Process ('Juiz.Process.processEntryPoint' と同じ関数とマニフェスト)
processPart :: forall f. (ProcessFn f, ToCapsule (FnResult f)) => ProcessManifest -> f -> ComponentPart
processPart m _ =
  ComponentPart (addProcess <$> checkProcessManifest m (Proxy :: Proxy f)) (pmTypeName m) (length (pmArguments m))
    (processArity (Proxy :: Proxy f))

-- | Container ('Juiz.Container.containerCreateEntryPoint' と同じ関数とマニフェスト)
containerPart :: forall f. ProcessFn f => ContainerManifest -> f -> ComponentPart
containerPart m _ =
  ComponentPart (addContainer m <$ checkArgumentTypes (cmArguments m) (argumentTypes (Proxy :: Proxy f)))
    (cmTypeName m) (length (cmArguments m)) (processArity (Proxy :: Proxy f))

-- | ContainerProcess ('Juiz.Container.containerProcessEntryPoint' と同じ関数とマニフェスト)
containerProcessPart :: forall s f. (ProcessFn f, ToCapsule (FnResult f)) => ProcessManifest -> (s -> f) -> ComponentPart
containerProcessPart m _ =
  ComponentPart (addProcess <$> checkProcessManifest m (Proxy :: Proxy f)) (pmTypeName m) (length (pmArguments m))
    (processArity (Proxy :: Proxy f))

-- | component_manifest_entry_point の中身。base に要素を足したマニフェストを書き込む。
-- 要素のマニフェストの引数の数と関数の引数の数が違う、ContainerProcess の Container が Component に無い、
-- 型名が重なっている、のどれかならエラーにする (juiz はプラグインを読み込むときに失敗する)
componentManifestEntryPoint :: ComponentManifest -> [ComponentPart] -> ManifestFunction
componentManifestEntryPoint base parts cp = guardEntryPoint (compTypeName base) $ do
  mapM_ checkArity parts
  adds <- mapM (\p -> either (\msg -> throwJuiz juizValueConverterError (partName p <> ": " <> msg)) pure (partAdd p)) parts
  case componentManifestToValue (foldl (flip ($)) base adds) of
    Left msg -> throwJuiz juizValueConverterError msg
    Right (JObject obj) -> writeCapsulePtr cp obj
    Right _ -> pure juizValueConverterError
  where
    checkArity p
      | partArguments p == partArity p = pure ()
      | otherwise = throwJuiz juizValueConverterError $
          partName p <> ": the manifest has " <> tshow (partArguments p)
            <> " arguments, but the function takes " <> tshow (partArity p)
