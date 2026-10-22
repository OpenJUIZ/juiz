{-# LANGUAGE FlexibleInstances #-}
{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE ScopedTypeVariables #-}
-- | Process のエントリポイント (C から呼ばれる関数) を Haskell の関数から作る。
--
-- ふつうは "Juiz.TH" の 'Juiz.TH.juizProcess' が、ここの関数を使って foreign export まで生成する。
-- Template Haskell を使わずに手で書く場合は次のようにする (README を参照)。
--
-- > increment :: Int64 -> IO Int64
-- > increment arg1 = pure (arg1 + 1)
-- >
-- > hsManifest :: ManifestFunction
-- > hsManifest = manifestEntryPoint manifest increment
-- > hsProcess :: ProcessFunction
-- > hsProcess = processEntryPoint manifest increment
-- > hsFactory :: ProcessFactoryFunction
-- > hsFactory = pure p_hsProcess
-- >
-- > foreign export ccall "manifest_entry_point" hsManifest :: ManifestFunction
-- > foreign export ccall "increment_process_entry_point" hsProcess :: ProcessFunction
-- > foreign import ccall "&increment_process_entry_point" p_hsProcess :: FunPtr ProcessFunction
-- > foreign export ccall "process_factory_entry_point" hsFactory :: ProcessFactoryFunction
module Juiz.Process
  ( ProcessFn (..)
  , manifestEntryPoint
  , processEntryPoint
  , guardEntryPoint
  ) where

import Control.Exception (SomeException, catch, displayException, evaluate, fromException, try)
import Data.Int (Int64)
import Data.Proxy (Proxy (..))
import Data.Text (Text)
import qualified Data.Text as T
import qualified Data.Text.IO as TIO
import System.IO (stderr)

import Juiz.Capsule
import Juiz.Error
import Juiz.Internal.FFI
import Juiz.Manifest
import Juiz.Value

-- | Process にできる関数: 引数が 'FromCapsule'、結果が @IO r@ ('ToCapsule' r) の関数。
-- 結果が IO でない関数は 'Juiz.TH.juizProcess' が @pure@ で包む
class ProcessFn f where
  -- | 引数の数
  processArity :: Proxy f -> Int
  -- | 引数 (名前と値の組) を順に渡して呼ぶ
  applyProcess :: f -> [(Text, Value)] -> IO Value

instance (FromCapsule a, ProcessFn b) => ProcessFn (a -> b) where
  processArity _ = 1 + processArity (Proxy :: Proxy b)
  applyProcess f ((name, v) : rest) = case fromValue v of
    Right a -> applyProcess (f a) rest
    Left msg -> throwJuiz juizValueConverterError ("argument \"" <> name <> "\": " <> msg)
  applyProcess _ [] = throwJuiz juizValueNotFoundError "too few arguments"

instance ToCapsule r => ProcessFn (IO r) where
  processArity _ = 0
  applyProcess act [] = do
    r <- act
    evaluate (toValue r)
  applyProcess _ _ = throwJuiz juizValueConverterError "too many arguments"

-- | manifest_entry_point の中身。マニフェストの引数の数と関数の引数の数が違えばエラーにする
-- (juiz はプラグインを読み込むときに失敗する)
manifestEntryPoint :: forall f. ProcessFn f => ProcessManifest -> f -> ManifestFunction
manifestEntryPoint m _ cp = guardEntryPoint (pmTypeName m) $ do
  let nArgs = length (pmArguments m)
      arity = processArity (Proxy :: Proxy f)
  if nArgs /= arity
    then throwJuiz juizValueConverterError $
      "the manifest has " <> tshow nArgs <> " arguments, but the function takes " <> tshow arity
    else case manifestToValue m of
      JObject obj -> writeCapsulePtr cp obj
      _ -> pure juizValueConverterError

-- | Process の関数の中身。マニフェストの引数を順に読み出して関数を呼び、結果を capsule に書き込む
processEntryPoint :: ProcessFn f => ProcessManifest -> f -> ProcessFunction
processEntryPoint m f cm out = guardEntryPoint (pmTypeName m) $ do
  args <- mapM (\a -> (,) (argName a) <$> readArgument cm (argName a)) (pmArguments m)
  result <- applyProcess f args
  writeCapsule out result

-- | Haskell の例外を C の呼び出し元に出さず、juiz のエラーコードに変える。
-- 'JuizError' はそのコード、それ以外の例外は 'juizProcessFunctionError'。内容は標準エラー出力に書く
guardEntryPoint :: Text -> IO Int64 -> IO Int64
guardEntryPoint name act = do
  r <- try (act >>= evaluate)
  case r of
    Right code -> pure code
    Left (e :: SomeException) -> do
      let (code, msg) = case fromException e of
            Just (JuizError c m) -> (c, m)
            Nothing -> (juizProcessFunctionError, T.pack (displayException e))
      TIO.hPutStrLn stderr ("hsjuiz: " <> name <> ": " <> msg <> " (error " <> tshow code <> ")")
        `catch` \(_ :: SomeException) -> pure ()
      pure code

tshow :: Show a => a -> Text
tshow = T.pack . show

