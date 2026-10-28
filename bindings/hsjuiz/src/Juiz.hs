-- | juiz の Process / Container を Haskell で書くためのライブラリ。
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
-- Container (状態を持つもの) と ContainerProcess (状態を第 1 引数に受け取る関数) は次のように書く。
--
-- > manifest :: ContainerManifest
-- > manifest = containerManifest "counter_container_hs"
-- >   & addIntArg "value" "initial value" 0
-- >
-- > create :: Int64 -> IO (IORef Int64)
-- > create = newIORef
-- >
-- > juizContainer 'create 'manifest
--
-- > manifest :: ProcessManifest
-- > manifest = containerProcessManifest "counter_container_hs_increment" "counter_container_hs"
-- >   & addIntArg "arg1" "amount" 1
-- >
-- > increment :: IORef Int64 -> Int64 -> IO Int64
-- > increment ref n = atomicModifyIORef' ref (\v -> (v + n, v + n))
-- >
-- > juizContainerProcess 'increment 'manifest
--
-- 1 つのライブラリに複数の Process / Container / ContainerProcess をまとめるときは Component にする。
-- 要素の関数とマニフェストは単体のときと同じものを使う。
--
-- > component :: ComponentManifest
-- > component = componentManifest "counter_component_hs"
-- >
-- > juizComponent 'component
-- >   [ componentProcess 'add 'addManifest
-- >   , componentContainer 'create 'counterManifest
-- >   , componentContainerProcess 'increment 'incrementManifest
-- >   ]
--
-- このモジュールを cabal の @foreign-library@ (type: native-shared、ghc-options: -threaded) に入れてビルドし、
-- juiz の設定ファイルに @language: "haskell"@ で書く。詳しくは bindings/hsjuiz/README.md。
module Juiz
  ( -- * Process / Container の公開
    juizProcess
  , juizContainer
  , juizContainerWithDestroy
  , juizContainerProcess
    -- * Component (1 つのライブラリにまとめる)
  , juizComponent
  , ComponentEntry
  , componentProcess
  , componentContainer
  , componentContainerWithDestroy
  , componentContainerProcess
    -- * マニフェスト
  , ProcessManifest
  , ContainerManifest
  , ComponentManifest
  , HasDescription
  , HasArguments
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
  , addImageArg
  , addTypedArg
  , addTypedArgValue
  , addTypedArgNoDefault
  , returns
  , (&)
    -- * 値
  , Value (..)
  , FromCapsule (..)
  , ToCapsule (..)
  , object
  , (.=)
    -- * 画像 (docs/spec/large-data.md §10)
  , Image (..)
  , PixelFormat (..)
  , bytesPerPixel
  , newImage
  , imageRowBytes
    -- * データ型 (juiz-idl -g haskell の生成した型、docs/spec/datatype.md §22)
  , JuizType (..)
  , TypeInfo
  , typeInfoName
  , typeInfoFingerprint
  , typeInfoSchema
  , unsafeTypeInfo
  , Proxy (..)
    -- * エラー
  , JuizError (..)
  , throwJuiz
  , juizHaskellContainerTypeError
  , juizValueDoesNotMatchTypeError
  , juizImageFormatError
    -- * よく使う型
  , Int64
  , Word64
  , Text
  , Map
    -- * Container の状態によく使う IORef
  , IORef
  , newIORef
  , readIORef
  , writeIORef
  , modifyIORef'
  , atomicModifyIORef'
    -- * TH を使わずに書くとき
  , ManifestFunction
  , ProcessFunction
  , ProcessFactoryFunction
  , ContainerCreateFunction
  , ContainerFactoryFunction
  , ContainerDestroyFunction
  , ContainerProcessFunction
  , ContainerProcessFactoryFunction
  , manifestEntryPoint
  , processEntryPoint
  , containerManifestEntryPoint
  , containerCreateEntryPoint
  , containerCreateEntryPointWithDestroy
  , containerDestroyEntryPoint
  , containerProcessManifestEntryPoint
  , containerProcessEntryPoint
  , ComponentPart
  , processPart
  , containerPart
  , containerProcessPart
  , componentManifestEntryPoint
  , FunPtr
  ) where

import Data.Function ((&))
import Data.Int (Int64)
import Data.IORef (IORef, atomicModifyIORef', modifyIORef', newIORef, readIORef, writeIORef)
import Data.Map.Strict (Map)
import Data.Proxy (Proxy (..))
import Data.Text (Text)
import Data.Word (Word64)
import Foreign.Ptr (FunPtr)

import Juiz.Capsule
import Juiz.Component
import Juiz.Container
import Juiz.Error
import Juiz.Image
import Juiz.Internal.FFI
  ( ContainerCreateFunction
  , ContainerDestroyFunction
  , ContainerFactoryFunction
  , ContainerProcessFactoryFunction
  , ContainerProcessFunction
  , ManifestFunction
  , ProcessFactoryFunction
  , ProcessFunction
  )
import Juiz.Manifest
import Juiz.Process
import Juiz.TH
import Juiz.Type
import Juiz.Value
