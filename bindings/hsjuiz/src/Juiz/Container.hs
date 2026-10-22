{-# LANGUAGE ExistentialQuantification #-}
{-# LANGUAGE FlexibleContexts #-}
{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE ScopedTypeVariables #-}
-- | Container と ContainerProcess のエントリポイント (C から呼ばれる関数) を Haskell の関数から作る。
--
-- ふつうは "Juiz.TH" の 'Juiz.TH.juizContainer' と 'Juiz.TH.juizContainerProcess' が、
-- ここの関数を使って foreign export まで生成する。Template Haskell を使わずに書く方法は README を参照。
--
-- Container の状態 (Container を作る関数の結果) は 'StablePtr' に包み、juiz には @void*@ として渡す。
-- juiz が Container を削除するときに呼ぶ解放関数 (container_factory_destroy_entry_point) で
-- 'freeStablePtr' するので、それまでは GC に回収されない。
module Juiz.Container
  ( containerManifestEntryPoint
  , containerCreateEntryPoint
  , containerCreateEntryPointWithDestroy
  , containerDestroyEntryPoint
  , containerProcessManifestEntryPoint
  , containerProcessEntryPoint
  ) where

import Control.Exception (finally)
import Control.Monad (unless, void, when)
import Data.Proxy (Proxy (..))
import qualified Data.Text as T
import Data.Typeable (TypeRep, Typeable, splitTyConApp, tyConModule, tyConName, typeOf, typeRep)
import Data.Word (Word64)
import Foreign.Marshal.Alloc (free, mallocBytes)
import Foreign.Ptr (Ptr, castPtr, nullPtr)
import Foreign.StablePtr (StablePtr, castStablePtrToPtr, deRefStablePtr, freeStablePtr, newStablePtr)
import Foreign.Storable (peek, peekByteOff, poke, pokeByteOff, sizeOf)
import Unsafe.Coerce (unsafeCoerce)

import Juiz.Capsule (ToCapsule)
import Juiz.Error
import Juiz.Internal.FFI
import Juiz.Manifest
import Juiz.Process

-- | juiz に渡す Container の中身: 状態と、解放するときに呼ぶ関数。
-- 状態の型は 'Typeable' で覚えておき、ContainerProcess に渡すときに確かめる
data ContainerBox = forall s. Typeable s => ContainerBox s (s -> IO ())

-- | juiz に渡す @void*@ は、先頭にこの値、続けて 'StablePtr' を置いた malloc の領域。
-- 別の言語の Container (C++ のオブジェクトなど) が ContainerProcess に渡されたときに、
-- StablePtr として読んでプロセスが落ちないよう、先頭の値で見分ける
containerMagic :: Word64
containerMagic = 0x48534a55495a4331 -- "HSJUIZC1"

magicSize :: Int
magicSize = sizeOf containerMagic

-- | Container の manifest_entry_point の中身。マニフェストの引数の数と、Container を作る関数の引数の数が
-- 違えばエラーにする (juiz はプラグインを読み込むときに失敗する)
containerManifestEntryPoint :: forall f. ProcessFn f => ContainerManifest -> f -> ManifestFunction
containerManifestEntryPoint m _ =
  checkedManifestEntryPoint (cmTypeName m) (length (cmArguments m)) (processArity (Proxy :: Proxy f))
    (containerManifestToValue m)

-- | Container を作る関数の中身。マニフェストの引数 (設定ファイルの containers に書いた値、
-- 無ければ既定値) を順に読み出して関数を呼び、結果を Container の状態にする
containerCreateEntryPoint :: (ProcessFn f, Typeable (FnResult f)) => ContainerManifest -> f -> ContainerCreateFunction
containerCreateEntryPoint m f = containerCreateEntryPointWithDestroy m f (\_ -> pure ())

-- | 'containerCreateEntryPoint' に、Container を削除するときに呼ぶ関数 (デバイスを閉じるなど) を足したもの
containerCreateEntryPointWithDestroy
  :: (ProcessFn f, Typeable (FnResult f))
  => ContainerManifest -> f -> (FnResult f -> IO ()) -> ContainerCreateFunction
containerCreateEntryPointWithDestroy m f destroy cm pobj = guardEntryPoint (cmTypeName m) $ do
  args <- readArguments (cmArguments m) cm
  s <- applyArgs f args
  cell <- mallocBytes (magicSize + sizeOf (undefined :: StablePtr ContainerBox))
  sp <- newStablePtr (ContainerBox s destroy)
  poke (castPtr cell) containerMagic
  pokeByteOff cell magicSize sp
  poke pobj cell
  pure juizOk

-- | container_factory_destroy_entry_point の中身。Container の状態を GC に返し、解放する関数を呼ぶ
containerDestroyEntryPoint :: ContainerManifest -> ContainerDestroyFunction
containerDestroyEntryPoint m p = void $ guardEntryPoint (cmTypeName m) $ do
  sp <- stablePtrOf p
  -- 二重に解放しないよう、先に印を消す
  poke (castPtr p) (0 :: Word64)
  ContainerBox s destroy <- deRefStablePtr sp
  (destroy s `finally` (freeStablePtr sp >> free p))
  pure juizOk

-- | ContainerProcess の manifest_entry_point の中身。
-- マニフェストの引数の数と、関数の (Container の状態を除いた) 引数の数が違えばエラーにする
containerProcessManifestEntryPoint :: forall s f. ProcessFn f => ProcessManifest -> (s -> f) -> ManifestFunction
containerProcessManifestEntryPoint m _ =
  checkedManifestEntryPoint (pmTypeName m) (length (pmArguments m)) (processArity (Proxy :: Proxy f))
    (manifestToValue m)

-- | ContainerProcess の関数の中身。第 1 引数に Container の状態を渡し、
-- 続けてマニフェストの引数を順に渡して呼ぶ。状態の型が違えば 'juizHaskellContainerTypeError'
containerProcessEntryPoint
  :: forall s f. (Typeable s, ProcessFn f, ToCapsule (FnResult f))
  => ProcessManifest -> (s -> f) -> ContainerProcessFunction
containerProcessEntryPoint m f p cm out = guardEntryPoint (pmTypeName m) $ do
  sp <- stablePtrOf p
  ContainerBox s _ <- deRefStablePtr sp
  let actual = typeOf s
      expected = typeRep (Proxy :: Proxy s)
  if typeKey actual == typeKey expected
    -- 型の名前 (モジュール名つき) が同じなら同じ型として扱う
    then applyAndWrite (pmArguments m) (f (unsafeCoerce s :: s)) cm out
    else throwJuiz juizHaskellContainerTypeError $
      "the container state is " <> T.pack (typeKey actual) <> ", but the process expects "
        <> T.pack (typeKey expected)

-- | 型を、パッケージを除いたモジュール名つきの名前で表す (@Data.IORef.IORef (GHC.Int.Int64)@ など)。
--
-- Container と ContainerProcess は別々の foreign-library になり、同じモジュールでもライブラリごとに
-- パッケージの識別子 (unit id) が違うことがあるので、'Data.Typeable.cast' (パッケージまで比べる) は使えない。
-- 両方のライブラリで同じモジュールの同じ型を使うこと (README を参照)
typeKey :: TypeRep -> String
typeKey t = tyConModule tc ++ "." ++ tyConName tc ++ concatMap (\a -> " (" ++ typeKey a ++ ")") args
  where
    (tc, args) = splitTyConApp t

-- | juiz から渡された @void*@ から 'StablePtr' を取り出す。hsjuiz の Container でなければエラー
stablePtrOf :: Ptr () -> IO (StablePtr ContainerBox)
stablePtrOf p = do
  when (p == nullPtr) $ throwJuiz juizHaskellContainerTypeError "the container is null"
  magic <- peek (castPtr p) :: IO Word64
  unless (magic == containerMagic) $
    throwJuiz juizHaskellContainerTypeError "the container is not a Haskell (hsjuiz) container"
  sp <- peekByteOff p magicSize
  when (castStablePtrToPtr sp == nullPtr) $ throwJuiz juizHaskellContainerTypeError "the container is broken"
  pure sp
