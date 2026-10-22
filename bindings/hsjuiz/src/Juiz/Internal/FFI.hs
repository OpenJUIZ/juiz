{-# LANGUAGE ForeignFunctionInterface #-}
-- | libjuiz_sdk の C 関数 (bindings/cjuiz/include/juiz_sdk.h) の宣言。
--
-- ふつうは "Juiz" の関数を使い、ここを直接使う必要はない。
-- 宣言は juiz_sdk.h と合わせること。callback で Haskell に戻ってくる関数は safe で呼ぶ。
module Juiz.Internal.FFI
  ( -- * 不透明な型
    CCapsulePtr, CCapsule, CCapsuleMap, CValue
    -- * プラグインが公開する関数の型
  , ProcessFunction, ManifestFunction, ProcessFactoryFunction
  , ContainerCreateFunction, ContainerFactoryFunction, ContainerDestroyFunction
  , ContainerProcessFunction, ContainerProcessFactoryFunction
    -- * callback の型
  , LockCallback, ObjectCallback, ArrayCallback
  , mkLockCallback, mkObjectCallback, mkArrayCallback
    -- * capsule_map
  , c_capsule_map_get_capsule
    -- * capsule_ptr
  , c_capsule_ptr_set_empty_object, c_capsule_ptr_lock_as_value_with_arg
    -- * capsule
  , c_capsule_set_null, c_capsule_set_bool, c_capsule_set_int, c_capsule_set_uint
  , c_capsule_set_float, c_capsule_set_string
  , c_capsule_set_empty_object, c_capsule_set_empty_array
  , c_capsule_lock_as_value_with_arg
    -- * value
  , c_hsjuiz_value_type
  , c_value_get_bool, c_value_get_int, c_value_get_uint, c_value_get_float
  , c_value_get_string_with_len
  , c_value_object_foreach, c_value_array_foreach
  , c_value_object_set_null, c_value_object_set_bool, c_value_object_set_int
  , c_value_object_set_uint, c_value_object_set_float, c_value_object_set_string
  , c_value_object_set_empty_object, c_value_object_set_empty_array
  , c_value_array_push_null, c_value_array_push_bool, c_value_array_push_int
  , c_value_array_push_uint, c_value_array_push_float, c_value_array_push_string
  , c_value_array_push_empty_object, c_value_array_push_empty_array
  ) where

import Data.Int (Int64)
import Data.Word (Word8, Word64)
import Foreign.C.String (CString)
import Foreign.C.Types (CSize (..))
import Foreign.Ptr (FunPtr, Ptr)

-- | juiz_sdk.h の capsule_ptr (Rust の CapsulePtr)。ポインタとしてだけ使う
data CCapsulePtr
-- | juiz_sdk.h の capsule (Rust の Capsule)。Process の戻り値の入れ物
data CCapsule
-- | juiz_sdk.h の capsule_map (Rust の CapsuleMap)。Process の引数
data CCapsuleMap
-- | juiz_sdk.h の value (Rust の Value)
data CValue

-- | Process の関数 (int64_t (*)(capsule_map*, capsule*))
type ProcessFunction = Ptr CCapsuleMap -> Ptr CCapsule -> IO Int64
-- | manifest_entry_point (int64_t (capsule_ptr*))
type ManifestFunction = Ptr CCapsulePtr -> IO Int64
-- | process_factory_entry_point (int64_t (*(void))(capsule_map*, capsule*))
type ProcessFactoryFunction = IO (FunPtr ProcessFunction)

-- | Container を作る関数 (int64_t (*)(capsule_map*, void**))。作った Container を void* で返す
type ContainerCreateFunction = Ptr CCapsuleMap -> Ptr (Ptr ()) -> IO Int64
-- | container_factory_entry_point (int64_t (*(void))(capsule_map*, void**))
type ContainerFactoryFunction = IO (FunPtr ContainerCreateFunction)
-- | container_factory_destroy_entry_point (void (void*))。Container を解放する
type ContainerDestroyFunction = Ptr () -> IO ()
-- | ContainerProcess の関数 (int64_t (*)(void*, capsule_map*, capsule*))。第 1 引数が Container
type ContainerProcessFunction = Ptr () -> Ptr CCapsuleMap -> Ptr CCapsule -> IO Int64
-- | container_process_factory_entry_point (int64_t (*(void))(void*, capsule_map*, capsule*))
type ContainerProcessFactoryFunction = IO (FunPtr ContainerProcessFunction)

-- | *_lock_as_value_with_arg の callback (int64_t (*)(void*, value*))
type LockCallback = Ptr () -> Ptr CValue -> IO Int64
-- | value_object_foreach の callback (void (*)(void*, const char*, value*))
type ObjectCallback = Ptr () -> CString -> Ptr CValue -> IO ()
-- | value_array_foreach の callback (void (*)(void*, value*))
type ArrayCallback = Ptr () -> Ptr CValue -> IO ()

foreign import ccall "wrapper" mkLockCallback :: LockCallback -> IO (FunPtr LockCallback)
foreign import ccall "wrapper" mkObjectCallback :: ObjectCallback -> IO (FunPtr ObjectCallback)
foreign import ccall "wrapper" mkArrayCallback :: ArrayCallback -> IO (FunPtr ArrayCallback)

-- capsule_map
foreign import ccall unsafe "capsule_map_get_capsule"
  c_capsule_map_get_capsule :: Ptr CCapsuleMap -> CString -> Ptr (Ptr CCapsulePtr) -> IO Int64

-- capsule_ptr
foreign import ccall unsafe "capsule_ptr_set_empty_object"
  c_capsule_ptr_set_empty_object :: Ptr CCapsulePtr -> IO Int64
foreign import ccall safe "capsule_ptr_lock_as_value_with_arg"
  c_capsule_ptr_lock_as_value_with_arg :: Ptr CCapsulePtr -> FunPtr LockCallback -> Ptr () -> IO Int64

-- capsule
foreign import ccall unsafe "capsule_set_null" c_capsule_set_null :: Ptr CCapsule -> IO Int64
foreign import ccall unsafe "capsule_set_bool" c_capsule_set_bool :: Ptr CCapsule -> Int64 -> IO Int64
foreign import ccall unsafe "capsule_set_int" c_capsule_set_int :: Ptr CCapsule -> Int64 -> IO Int64
foreign import ccall unsafe "capsule_set_uint" c_capsule_set_uint :: Ptr CCapsule -> Word64 -> IO Int64
foreign import ccall unsafe "capsule_set_float" c_capsule_set_float :: Ptr CCapsule -> Double -> IO Int64
foreign import ccall unsafe "capsule_set_string" c_capsule_set_string :: Ptr CCapsule -> CString -> IO Int64
foreign import ccall unsafe "capsule_set_empty_object" c_capsule_set_empty_object :: Ptr CCapsule -> IO Int64
foreign import ccall unsafe "capsule_set_empty_array" c_capsule_set_empty_array :: Ptr CCapsule -> IO Int64
foreign import ccall safe "capsule_lock_as_value_with_arg"
  c_capsule_lock_as_value_with_arg :: Ptr CCapsule -> FunPtr LockCallback -> Ptr () -> IO Int64

-- value (読み出し)
foreign import ccall unsafe "hsjuiz_value_type" c_hsjuiz_value_type :: Ptr CValue -> IO Int64
-- value_get_bool は bool* (1 バイト) に書き込む
foreign import ccall unsafe "value_get_bool" c_value_get_bool :: Ptr CValue -> Ptr Word8 -> IO Int64
foreign import ccall unsafe "value_get_int" c_value_get_int :: Ptr CValue -> Ptr Int64 -> IO Int64
foreign import ccall unsafe "value_get_uint" c_value_get_uint :: Ptr CValue -> Ptr Word64 -> IO Int64
foreign import ccall unsafe "value_get_float" c_value_get_float :: Ptr CValue -> Ptr Double -> IO Int64
foreign import ccall unsafe "value_get_string_with_len"
  c_value_get_string_with_len :: Ptr CValue -> Ptr CString -> Ptr CSize -> IO Int64
foreign import ccall safe "value_object_foreach"
  c_value_object_foreach :: Ptr CValue -> FunPtr ObjectCallback -> Ptr () -> IO Int64
foreign import ccall safe "value_array_foreach"
  c_value_array_foreach :: Ptr CValue -> FunPtr ArrayCallback -> Ptr () -> IO Int64

-- value (オブジェクトへの書き込み。入れた要素を返す)
foreign import ccall unsafe "value_object_set_null" c_value_object_set_null :: Ptr CValue -> CString -> IO (Ptr CValue)
foreign import ccall unsafe "value_object_set_bool" c_value_object_set_bool :: Ptr CValue -> CString -> Int64 -> IO (Ptr CValue)
foreign import ccall unsafe "value_object_set_int" c_value_object_set_int :: Ptr CValue -> CString -> Int64 -> IO (Ptr CValue)
foreign import ccall unsafe "value_object_set_uint" c_value_object_set_uint :: Ptr CValue -> CString -> Word64 -> IO (Ptr CValue)
foreign import ccall unsafe "value_object_set_float" c_value_object_set_float :: Ptr CValue -> CString -> Double -> IO (Ptr CValue)
foreign import ccall unsafe "value_object_set_string" c_value_object_set_string :: Ptr CValue -> CString -> CString -> IO (Ptr CValue)
foreign import ccall unsafe "value_object_set_empty_object" c_value_object_set_empty_object :: Ptr CValue -> CString -> IO (Ptr CValue)
foreign import ccall unsafe "value_object_set_empty_array" c_value_object_set_empty_array :: Ptr CValue -> CString -> IO (Ptr CValue)

-- value (配列への追加。足した要素を返す)
foreign import ccall unsafe "value_array_push_null" c_value_array_push_null :: Ptr CValue -> IO (Ptr CValue)
foreign import ccall unsafe "value_array_push_bool" c_value_array_push_bool :: Ptr CValue -> Int64 -> IO (Ptr CValue)
foreign import ccall unsafe "value_array_push_int" c_value_array_push_int :: Ptr CValue -> Int64 -> IO (Ptr CValue)
foreign import ccall unsafe "value_array_push_uint" c_value_array_push_uint :: Ptr CValue -> Word64 -> IO (Ptr CValue)
foreign import ccall unsafe "value_array_push_float" c_value_array_push_float :: Ptr CValue -> Double -> IO (Ptr CValue)
foreign import ccall unsafe "value_array_push_string" c_value_array_push_string :: Ptr CValue -> CString -> IO (Ptr CValue)
foreign import ccall unsafe "value_array_push_empty_object" c_value_array_push_empty_object :: Ptr CValue -> IO (Ptr CValue)
foreign import ccall unsafe "value_array_push_empty_array" c_value_array_push_empty_array :: Ptr CValue -> IO (Ptr CValue)
