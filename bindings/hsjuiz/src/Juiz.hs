-- | juiz の Process を Haskell で書くためのライブラリ。
--
-- > {-# LANGUAGE OverloadedStrings #-}
-- > {-# LANGUAGE TemplateHaskell #-}
-- > module Increment () where
-- >
-- > import Juiz
-- >
-- > manifest :: ProcessManifest
-- > manifest = processManifest "increment_process_hs"
-- >   & description "Adds 1 to arg1."
-- >   & addIntArg "arg1" "value to be incremented" 1
-- >
-- > increment :: Int64 -> IO Int64
-- > increment arg1 = pure (arg1 + 1)
-- >
-- > juizProcess 'increment 'manifest
--
-- このモジュールを cabal の @foreign-library@ (type: native-shared、ghc-options: -threaded) に入れてビルドし、
-- juiz の設定ファイルに @language: "haskell"@ で書く。詳しくは bindings/hsjuiz/README.md。
module Juiz
  ( -- * Process の公開
    juizProcess
    -- * マニフェスト
  , ProcessManifest
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
  , (&)
    -- * 値
  , Value (..)
  , FromCapsule (..)
  , ToCapsule (..)
  , object
  , (.=)
    -- * エラー
  , JuizError (..)
  , throwJuiz
    -- * よく使う型
  , Int64
  , Word64
  , Text
  , Map
    -- * TH を使わずに書くとき
  , ManifestFunction
  , ProcessFunction
  , ProcessFactoryFunction
  , manifestEntryPoint
  , processEntryPoint
  , FunPtr
  ) where

import Data.Function ((&))
import Data.Int (Int64)
import Data.Map.Strict (Map)
import Data.Text (Text)
import Data.Word (Word64)
import Foreign.Ptr (FunPtr)

import Juiz.Capsule
import Juiz.Error
import Juiz.Internal.FFI (ManifestFunction, ProcessFactoryFunction, ProcessFunction)
import Juiz.Manifest
import Juiz.Process
import Juiz.TH
import Juiz.Value
