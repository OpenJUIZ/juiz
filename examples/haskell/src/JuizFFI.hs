{-# LANGUAGE ForeignFunctionInterface #-}
-- | libjuiz_sdk の C 関数 (bindings/cjuiz/include/juiz_sdk.h) を手書きの FFI で呼ぶための最小限の宣言。
-- Step 2 (issue #132) でライブラリ hsjuiz に置き換える。
module JuizFFI
  ( CapsulePtr, Capsule, CapsuleMap, Value
  , ProcessFunction, ManifestBuilder
  , juizOk, juizValueNotFoundError, juizValueConverterError
  , getIntArg, setIntResult
  , writeManifest, setString, setInt, pushObject, setEmptyArray
  , guardException
  , mkManifestBuilder
  ) where

import Control.Exception (SomeException, try, evaluate)
import Data.Int (Int64)
import Foreign
import Foreign.C.String

-- 中身は Rust 側の型。ポインタとしてだけ使う
data CapsulePtr
data Capsule
data CapsuleMap
data Value

-- | Process の関数の型 (juiz_sdk.h の int64_t (*)(capsule_map*, capsule*))
type ProcessFunction = Ptr CapsuleMap -> Ptr Capsule -> IO Int64

-- | マニフェストを書き込む関数の型 (capsule_ptr_lock_as_value_with_arg の callback)
type ManifestBuilder = Ptr () -> Ptr Value -> IO Int64

juizOk, juizValueNotFoundError, juizValueConverterError :: Int64
juizOk = 0
juizValueNotFoundError = -102
juizValueConverterError = -103

foreign import ccall unsafe "capsule_map_get_capsule"
  c_capsule_map_get_capsule :: Ptr CapsuleMap -> CString -> Ptr (Ptr CapsulePtr) -> IO Int64
foreign import ccall unsafe "capsule_ptr_get_int"
  c_capsule_ptr_get_int :: Ptr CapsulePtr -> Ptr Int64 -> IO Int64
foreign import ccall unsafe "capsule_set_int"
  c_capsule_set_int :: Ptr Capsule -> Int64 -> IO Int64
foreign import ccall unsafe "capsule_ptr_set_empty_object"
  c_capsule_ptr_set_empty_object :: Ptr CapsulePtr -> IO Int64
-- callback で Haskell に戻ってくるので safe で呼ぶ
foreign import ccall safe "capsule_ptr_lock_as_value_with_arg"
  c_capsule_ptr_lock_as_value_with_arg :: Ptr CapsulePtr -> FunPtr ManifestBuilder -> Ptr () -> IO Int64
foreign import ccall unsafe "value_object_set_string"
  c_value_object_set_string :: Ptr Value -> CString -> CString -> IO (Ptr Value)
foreign import ccall unsafe "value_object_set_int"
  c_value_object_set_int :: Ptr Value -> CString -> Int64 -> IO (Ptr Value)
foreign import ccall unsafe "value_object_set_empty_array"
  c_value_object_set_empty_array :: Ptr Value -> CString -> IO (Ptr Value)
foreign import ccall unsafe "value_array_push_empty_object"
  c_value_array_push_empty_object :: Ptr Value -> IO (Ptr Value)

foreign import ccall "wrapper"
  mkManifestBuilder :: ManifestBuilder -> IO (FunPtr ManifestBuilder)

-- | 引数 name を Int64 として取り出す。失敗したら juiz のエラーコードを返す
getIntArg :: Ptr CapsuleMap -> String -> IO (Either Int64 Int64)
getIntArg cm name =
  withCString name $ \cname -> alloca $ \pcp -> do
    r <- c_capsule_map_get_capsule cm cname pcp
    if r /= juizOk then pure (Left r) else do
      cp <- peek pcp
      alloca $ \pv -> do
        r' <- c_capsule_ptr_get_int cp pv
        if r' /= juizOk then pure (Left r') else Right <$> peek pv

setIntResult :: Ptr Capsule -> Int64 -> IO Int64
setIntResult = c_capsule_set_int

-- | capsule_ptr を空のオブジェクトにしてから、builder で中身を書き込む
writeManifest :: Ptr CapsulePtr -> (Ptr Value -> IO ()) -> IO Int64
writeManifest cp build = do
  r <- c_capsule_ptr_set_empty_object cp
  if r /= juizOk then pure r else do
    fp <- mkManifestBuilder (\_ v -> build v >> pure juizOk)
    r' <- c_capsule_ptr_lock_as_value_with_arg cp fp nullPtr
    freeHaskellFunPtr fp
    pure r'

setString :: Ptr Value -> String -> String -> IO ()
setString v k s = withCString k $ \ck -> withCString s $ \cs -> () <$ c_value_object_set_string v ck cs

setInt :: Ptr Value -> String -> Int64 -> IO ()
setInt v k i = withCString k $ \ck -> () <$ c_value_object_set_int v ck i

setEmptyArray :: Ptr Value -> String -> IO (Ptr Value)
setEmptyArray v k = withCString k $ \ck -> c_value_object_set_empty_array v ck

pushObject :: Ptr Value -> IO (Ptr Value)
pushObject = c_value_array_push_empty_object

-- | Haskell の例外を C の呼び出し元に出さず、エラーコードに変える
guardException :: IO Int64 -> IO Int64
guardException act = do
  r <- try (act >>= evaluate) :: IO (Either SomeException Int64)
  pure (either (const juizValueConverterError) id r)
