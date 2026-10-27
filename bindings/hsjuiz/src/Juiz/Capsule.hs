{-# LANGUAGE FlexibleInstances #-}
{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE ScopedTypeVariables #-}
-- | juiz の値と Haskell の値の変換 ('FromCapsule' / 'ToCapsule') と、libjuiz_sdk の値の読み書き。
module Juiz.Capsule
  ( -- * 変換の型クラス
    FromCapsule (..)
  , ToCapsule (..)
  , object
  , (.=)
    -- * libjuiz_sdk の値の読み書き
  , peekValue
  , readArgument
  , readTypedArgument
  , writeCapsule
  , writeCapsulePtr
  ) where

import Control.Exception (SomeException, throwIO, try)
import Control.Monad (forM_, unless, void)
import qualified Data.ByteString as BS
import Data.IORef
import Data.Int (Int16, Int32, Int64, Int8)
import Data.Map.Strict (Map)
import qualified Data.Map.Strict as Map
import Data.Text (Text)
import qualified Data.Text as T
import qualified Data.Text.Encoding as TE
import Data.Word (Word16, Word32, Word64, Word8)
import Foreign.C.String (CString)
import Foreign.C.Types (CSize)
import Foreign.Marshal.Alloc (alloca)
import Data.Proxy (Proxy)
import Foreign.Ptr (FunPtr, Ptr, freeHaskellFunPtr, nullPtr)
import Foreign.Storable (Storable, peek)

import Juiz.Error
import Juiz.Internal.Base64
import Juiz.Internal.FFI
import Juiz.Internal.TypeInfo
import Juiz.Value

-- | juiz の値から Haskell の値への変換。Process の引数の型はこのクラスのインスタンスにする
class FromCapsule a where
  -- | 変換できなければ理由を 'Left' で返す
  fromValue :: Value -> Either Text a
  -- | 型付きの型 (juiz-idl -g haskell の生成した型など、docs/spec/datatype.md §22) ならその型。
  -- 型付きの引数は、juiz が型で読んだ値 (足りないフィールドを既定値で埋めたもの) を受け取る
  fromCapsuleType :: Proxy a -> Maybe TypeInfo
  fromCapsuleType _ = Nothing

-- | Haskell の値から juiz の値への変換。Process の戻り値の型はこのクラスのインスタンスにする
class ToCapsule a where
  toValue :: a -> Value
  -- | 型付きの型ならその型。Process の結果がこの型なら、manifest に output_schema を載せる
  toCapsuleType :: Proxy a -> Maybe TypeInfo
  toCapsuleType _ = Nothing

instance FromCapsule Value where fromValue = Right
instance ToCapsule Value where toValue = id

instance FromCapsule Bool where
  fromValue (JBool b) = Right b
  fromValue v = typeMismatch "Bool" v
instance ToCapsule Bool where toValue = JBool

instance FromCapsule Int64 where
  fromValue (JInt i) = Right i
  fromValue v = typeMismatch "Int64" v
instance ToCapsule Int64 where toValue = JInt

instance FromCapsule Word64 where
  fromValue (JUInt w) = Right w
  fromValue (JInt i) | i >= 0 = Right (fromIntegral i)
  fromValue v = typeMismatch "Word64" v
instance ToCapsule Word64 where
  toValue w
    | w <= fromIntegral (maxBound :: Int64) = JInt (fromIntegral w)
    | otherwise = JUInt w

instance FromCapsule Int where fromValue = boundedInt "Int"
instance ToCapsule Int where toValue = JInt . fromIntegral
instance FromCapsule Int8 where fromValue = boundedInt "Int8"
instance ToCapsule Int8 where toValue = JInt . fromIntegral
instance FromCapsule Int16 where fromValue = boundedInt "Int16"
instance ToCapsule Int16 where toValue = JInt . fromIntegral
instance FromCapsule Int32 where fromValue = boundedInt "Int32"
instance ToCapsule Int32 where toValue = JInt . fromIntegral
instance FromCapsule Word8 where fromValue = boundedInt "Word8"
instance ToCapsule Word8 where toValue = JInt . fromIntegral
instance FromCapsule Word16 where fromValue = boundedInt "Word16"
instance ToCapsule Word16 where toValue = JInt . fromIntegral
instance FromCapsule Word32 where fromValue = boundedInt "Word32"
instance ToCapsule Word32 where toValue = JInt . fromIntegral

-- | 整数も小数として受け取る
instance FromCapsule Double where
  fromValue (JFloat d) = Right d
  fromValue (JInt i) = Right (fromIntegral i)
  fromValue (JUInt w) = Right (fromIntegral w)
  fromValue v = typeMismatch "Double" v
instance ToCapsule Double where toValue = JFloat

-- | 整数も小数として受け取る (データ型の float)
instance FromCapsule Float where
  fromValue v = realToFrac <$> (fromValue v :: Either Text Double)
instance ToCapsule Float where toValue = JFloat . realToFrac

instance FromCapsule Text where
  fromValue (JString t) = Right t
  fromValue v = typeMismatch "Text" v
instance ToCapsule Text where toValue = JString

-- | バイト列は base64 (RFC 4648 §4、パディングあり) の文字列で表す (データ型の bytes、docs/spec/datatype.md §4)
instance FromCapsule BS.ByteString where
  fromValue (JString t) = maybe (Left "invalid base64 string") Right (decodeBase64 (T.unpack t))
  fromValue v = typeMismatch "ByteString (base64 string)" v
instance ToCapsule BS.ByteString where toValue = JString . T.pack . encodeBase64

-- | 配列。文字列は 'String' ではなく 'Text' で受け取る
instance FromCapsule a => FromCapsule [a] where
  fromValue (JArray xs) = traverse fromValue xs
  fromValue v = typeMismatch "Array" v
instance ToCapsule a => ToCapsule [a] where toValue = JArray . map toValue

-- | オブジェクト
instance FromCapsule a => FromCapsule (Map Text a) where
  fromValue (JObject m) = traverse fromValue m
  fromValue v = typeMismatch "Object" v
instance ToCapsule a => ToCapsule (Map Text a) where toValue = JObject . Map.map toValue

-- | null は 'Nothing'
instance FromCapsule a => FromCapsule (Maybe a) where
  fromValue JNull = Right Nothing
  fromValue v = Just <$> fromValue v
instance ToCapsule a => ToCapsule (Maybe a) where toValue = maybe JNull toValue

-- | 何も返さない Process の戻り値は null になる
instance ToCapsule () where toValue () = JNull

-- | キーと値の組からオブジェクトを作る。
--
-- > object ["x" .= (1 :: Int64), "name" .= ("abc" :: Text)]
object :: [(Text, Value)] -> Value
object = JObject . Map.fromList

(.=) :: ToCapsule a => Text -> a -> (Text, Value)
k .= v = (k, toValue v)
infixr 8 .=

typeMismatch :: Text -> Value -> Either Text a
typeMismatch expected v = Left ("expected " <> expected <> ", but got " <> T.pack (show v))

boundedInt :: forall a. (Integral a, Bounded a) => Text -> Value -> Either Text a
boundedInt name v = case v of
  JInt i | toInteger i >= lo && toInteger i <= hi -> Right (fromIntegral i)
  JUInt w | toInteger w <= hi -> Right (fromIntegral w)
  JInt _ -> outOfRange
  JUInt _ -> outOfRange
  _ -> typeMismatch name v
  where
    lo = toInteger (minBound :: a)
    hi = toInteger (maxBound :: a)
    outOfRange = Left (T.pack (show v) <> " is out of range of " <> name)

-- ---------------------------------------------------------------------------
-- libjuiz_sdk の値の読み書き

-- | callback (C から呼ばれる Haskell の関数) を作って act に渡し、終わったら解放する。
-- callback の中の例外は C 側に出さずに覚えておき、act の後で投げ直す
withCallback
  :: (cb -> IO (FunPtr cb))
  -> ((IO () -> IO ()) -> cb)  -- ^ 例外を捕まえる関数を受け取って callback を作る
  -> (FunPtr cb -> IO r)
  -> IO r
withCallback mk build act = do
  failure <- newIORef Nothing
  let guarded body = do
        r <- try body
        case r of
          Left (e :: SomeException) -> modifyIORef' failure (maybe (Just e) Just)
          Right () -> pure ()
  fp <- mk (build guarded)
  r <- act fp
  freeHaskellFunPtr fp
  readIORef failure >>= maybe (pure r) throwIO

-- | lock_as_value_with_arg の callback を作って呼ぶ。callback が書き込みに失敗したら例外になる
withLockCallback :: (Ptr CValue -> IO ()) -> (FunPtr LockCallback -> Ptr () -> IO Int64) -> IO Int64
withLockCallback body lock =
  withCallback mkLockCallback (\guarded _ v -> juizOk <$ guarded (body v)) (\fp -> lock fp nullPtr)

-- | value の中身を 'Value' として読み出す (lock の callback の中で呼ぶ)
peekValue :: Ptr CValue -> IO Value
peekValue v = do
  ty <- c_hsjuiz_value_type v
  case ty of
    0 -> pure JNull
    1 -> JBool . (/= 0) <$> getWith c_value_get_bool
    2 -> JInt <$> getWith c_value_get_int
    3 -> JUInt <$> getWith c_value_get_uint
    4 -> JFloat <$> getWith c_value_get_float
    5 -> alloca $ \pstr -> alloca $ \plen -> do
      check "value_get_string_with_len" =<< c_value_get_string_with_len v pstr plen
      str <- peek pstr
      len <- peek plen :: IO CSize
      bytes <- BS.packCStringLen (str, fromIntegral len)
      pure (JString (TE.decodeUtf8 bytes))
    6 -> do
      items <- newIORef []
      _ <- withCallback mkArrayCallback
        (\guarded _ e -> guarded (peekValue e >>= \x -> modifyIORef' items (x :)))
        (\fp -> c_value_array_foreach v fp nullPtr)
      JArray . reverse <$> readIORef items
    7 -> do
      items <- newIORef Map.empty
      _ <- withCallback mkObjectCallback
        (\guarded _ k e -> guarded $ do
            key <- peekText k
            x <- peekValue e
            modifyIORef' items (Map.insert key x))
        (\fp -> c_value_object_foreach v fp nullPtr)
      JObject <$> readIORef items
    _ -> throwJuiz juizValueTypeError "unknown value type"
  where
    getWith :: Storable a => (Ptr CValue -> Ptr a -> IO Int64) -> IO a
    getWith f = alloca $ \p -> do
      check "value_get" =<< f v p
      peek p

peekText :: CString -> IO Text
peekText s = TE.decodeUtf8 <$> BS.packCString s

-- | NUL 終端の UTF-8 の文字列として C に渡す (途中に NUL があればそこまでになる)
withText :: Text -> (CString -> IO a) -> IO a
withText t = BS.useAsCString (TE.encodeUtf8 t)

check :: Text -> Int64 -> IO ()
check what r = unless (r == juizOk) $ throwJuiz r (what <> " failed")

checkPtr :: Text -> Ptr CValue -> IO (Ptr CValue)
checkPtr what p
  | p == nullPtr = throwJuiz juizValueConverterError (what <> " failed")
  | otherwise = pure p

-- | Process の引数 name を読み出す。無ければ 'juizValueNotFoundError' の 'JuizError' を投げる
readArgument :: Ptr CCapsuleMap -> Text -> IO Value
readArgument cm name =
  withText name $ \cname -> alloca $ \pcp -> do
    r <- c_capsule_map_get_capsule cm cname pcp
    unless (r == juizOk) $ throwJuiz r ("argument \"" <> name <> "\" not found")
    cp <- peek pcp
    result <- newIORef JNull
    r' <- withLockCallback (\v -> peekValue v >>= writeIORef result) (c_capsule_ptr_lock_as_value_with_arg cp)
    unless (r' == juizOk) $ throwJuiz r' ("argument \"" <> name <> "\" is not a value")
    readIORef result

-- | 型付きの引数 name を型 t で読み出す (docs/spec/datatype.md §4。足りないフィールドは既定値で埋め、
-- 型にないキーは落とす)。無ければ 'juizValueNotFoundError'、型に合わなければ 'juizValueDoesNotMatchTypeError'
readTypedArgument :: Ptr CCapsuleMap -> Text -> TypeInfo -> IO Value
readTypedArgument cm name t =
  withText name $ \cname -> alloca $ \pcp -> do
    r <- c_capsule_map_get_capsule cm cname pcp
    unless (r == juizOk) $ throwJuiz r ("argument \"" <> name <> "\" not found")
    cp <- peek pcp
    result <- newIORef JNull
    r' <- withLockCallback (\v -> peekValue v >>= writeIORef result)
      (\fp arg -> c_capsule_ptr_read_typed_value cp (typeInfoHandle t) cname fp arg)
    unless (r' == juizOk) $
      throwJuiz r' ("argument \"" <> name <> "\" does not match the type " <> typeInfoName t)
    readIORef result

-- | オブジェクトの要素を書き込む
setMember :: Ptr CValue -> Text -> Value -> IO ()
setMember obj key x = withText key $ \ck -> case x of
  JNull -> void . checkPtr "value_object_set_null" =<< c_value_object_set_null obj ck
  JBool b -> void . checkPtr "value_object_set_bool" =<< c_value_object_set_bool obj ck (fromBool b)
  JInt i -> void . checkPtr "value_object_set_int" =<< c_value_object_set_int obj ck i
  JUInt w -> void . checkPtr "value_object_set_uint" =<< c_value_object_set_uint obj ck w
  JFloat d -> void . checkPtr "value_object_set_float" =<< c_value_object_set_float obj ck d
  JString t -> withText t $ \cs -> void . checkPtr "value_object_set_string" =<< c_value_object_set_string obj ck cs
  JArray xs -> do
    a <- checkPtr "value_object_set_empty_array" =<< c_value_object_set_empty_array obj ck
    forM_ xs (pushElement a)
  JObject m -> do
    o <- checkPtr "value_object_set_empty_object" =<< c_value_object_set_empty_object obj ck
    forM_ (Map.toList m) (uncurry (setMember o))

-- | 配列に要素を足す
pushElement :: Ptr CValue -> Value -> IO ()
pushElement arr x = case x of
  JNull -> void . checkPtr "value_array_push_null" =<< c_value_array_push_null arr
  JBool b -> void . checkPtr "value_array_push_bool" =<< c_value_array_push_bool arr (fromBool b)
  JInt i -> void . checkPtr "value_array_push_int" =<< c_value_array_push_int arr i
  JUInt w -> void . checkPtr "value_array_push_uint" =<< c_value_array_push_uint arr w
  JFloat d -> void . checkPtr "value_array_push_float" =<< c_value_array_push_float arr d
  JString t -> withText t $ \cs -> void . checkPtr "value_array_push_string" =<< c_value_array_push_string arr cs
  JArray xs -> do
    a <- checkPtr "value_array_push_empty_array" =<< c_value_array_push_empty_array arr
    forM_ xs (pushElement a)
  JObject m -> do
    o <- checkPtr "value_array_push_empty_object" =<< c_value_array_push_empty_object arr
    forM_ (Map.toList m) (uncurry (setMember o))

-- | 中身が空のオブジェクト・配列の value に要素を書き込む
fill :: Value -> Ptr CValue -> IO ()
fill (JArray xs) v = forM_ xs (pushElement v)
fill (JObject m) v = forM_ (Map.toList m) (uncurry (setMember v))
fill _ _ = pure ()

fromBool :: Bool -> Int64
fromBool b = if b then 1 else 0

-- | Process の戻り値 (capsule) に書き込む。戻り値は juiz のエラーコード
writeCapsule :: Ptr CCapsule -> Value -> IO Int64
writeCapsule cap x = case x of
  JNull -> c_capsule_set_null cap
  JBool b -> c_capsule_set_bool cap (fromBool b)
  JInt i -> c_capsule_set_int cap i
  JUInt w -> c_capsule_set_uint cap w
  JFloat d -> c_capsule_set_float cap d
  JString t -> withText t (c_capsule_set_string cap)
  JArray _ -> c_capsule_set_empty_array cap `andThen` lock
  JObject _ -> c_capsule_set_empty_object cap `andThen` lock
  where
    lock = withLockCallback (fill x) (c_capsule_lock_as_value_with_arg cap)

-- | capsule_ptr にオブジェクトを書き込む (マニフェストを返すのに使う)
writeCapsulePtr :: Ptr CCapsulePtr -> Map Text Value -> IO Int64
writeCapsulePtr cp m =
  c_capsule_ptr_set_empty_object cp `andThen`
    withLockCallback (fill (JObject m)) (c_capsule_ptr_lock_as_value_with_arg cp)

andThen :: IO Int64 -> IO Int64 -> IO Int64
andThen first second = do
  r <- first
  if r == juizOk then second else pure r

