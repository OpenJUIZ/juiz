{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE ScopedTypeVariables #-}
-- | Process の引数・戻り値のデータ型 (docs/spec/datatype.md §22)。
--
-- ふつうは @juiz-idl -g haskell@ が .jidl から作ったモジュールの型 (record と直和型) を使う。
-- 生成した型は 'JuizType' と 'FromCapsule' / 'ToCapsule' のインスタンスで、Process の関数の引数・結果に書くと
-- 型付きの引数・戻り値になる (manifest には 'Juiz.Manifest.addTypedArg' で足す)。
--
-- 手で書いた型を型付きにするときは、Avro のスキーマから 'TypeInfo' を作り、インスタンスを書く。
--
-- > data Odometry = Odometry { odometryDistance :: Double, odometrySteps :: Int64 }
-- >
-- > odometryType :: TypeInfo
-- > odometryType = unsafeTypeInfo "{\"type\": \"record\", \"name\": \"Odometry\", \"namespace\": \"my\", \"fields\": [...]}"
-- > {-# NOINLINE odometryType #-}
-- >
-- > instance JuizType Odometry where juizType _ = odometryType
-- > instance FromCapsule Odometry where
-- >   fromValue v = recordFromValue "my.Odometry" v $ \o -> Odometry <$> field o "distance" Nothing <*> field o "steps" Nothing
-- >   fromCapsuleType = Just . juizType
-- > instance ToCapsule Odometry where
-- >   toValue x = object ["distance" .= odometryDistance x, "steps" .= odometrySteps x]
-- >   toCapsuleType = Just . juizType
module Juiz.Type
  ( TypeInfo
  , JuizType (..)
  , typeInfoSchema
  , typeInfoTypeName
  , typeInfoName
  , typeInfoFingerprint
  , newTypeInfo
  , unsafeTypeInfo
    -- * 生成したコードが使う関数
  , recordFromValue
  , field
  , enumFromValue
  ) where

import Control.Exception (bracket)
import Data.IORef (newIORef, readIORef, writeIORef)
import qualified Data.ByteString as BS
import Data.Map.Strict (Map)
import qualified Data.Map.Strict as Map
import Data.Proxy (Proxy)
import Data.Text (Text)
import qualified Data.Text as T
import qualified Data.Text.Encoding as TE
import Foreign.Marshal.Alloc (alloca)
import Foreign.StablePtr (castStablePtrToPtr, freeStablePtr, newStablePtr)
import Foreign.Storable (peek)
import System.IO.Unsafe (unsafePerformIO)

import Juiz.Capsule
import Juiz.Error
import Juiz.Internal.FFI
import Juiz.Internal.TypeInfo
import Juiz.Value

-- | 型付きの型。生成した型はすべてこのインスタンス
class (FromCapsule a, ToCapsule a) => JuizType a where
  juizType :: Proxy a -> TypeInfo

-- | Avro のスキーマ (.avsc の JSON) から型を作る。juiz のデータ型の規則 (仕様 §3) に合わなければ
-- 'juizTypeSchemaInvalidError' の 'JuizError' を投げる (理由は juiz のログに出る)。
-- 作った型は解放しない (プラグインが読み込まれている間使う)
newTypeInfo :: Text -> IO TypeInfo
newTypeInfo avro = BS.useAsCString (TE.encodeUtf8 avro) $ \cs -> alloca $ \pt -> do
  r <- c_juiz_type_new cs pt
  if r /= juizOk
    then throwJuiz r ("invalid type schema: " <> T.take 200 avro)
    else do
      t <- peek pt
      result <- newIORef JNull
      r' <- bracket (newStablePtr ((\v -> juizOk <$ (peekValue v >>= writeIORef result)) :: LockFunction)) freeStablePtr
        (c_juiz_type_describe t lockTrampoline . castStablePtrToPtr)
      if r' /= juizOk
        then throwJuiz r' "juiz_type_describe failed"
        else do
          desc <- readIORef result
          case desc of
            JObject m
              | Just schema <- Map.lookup "schema" m
              , Just (JString typeName) <- Map.lookup "type_name" m ->
                  pure (TypeInfo t schema typeName)
            _ -> throwJuiz juizValueConverterError "juiz_type_describe returned an unexpected value"

-- | トップレベルの値として型を作る ('newTypeInfo' を 'unsafePerformIO' で呼ぶ)。
-- 型ごとに 1 度だけ作るよう、@{-# NOINLINE … #-}@ を付けたトップレベルの値にすること
unsafeTypeInfo :: Text -> TypeInfo
unsafeTypeInfo = unsafePerformIO . newTypeInfo

-- | record を辞書 (オブジェクト) から読む。オブジェクトでなければ Left
recordFromValue :: Text -> Value -> (Map Text Value -> Either Text a) -> Either Text a
recordFromValue _ (JObject o) build = build o
recordFromValue name v _ = Left ("expected " <> name <> " (object), but got " <> T.pack (show v))

-- | record のフィールドを読む (仕様 §4)。キーが無ければ既定値 (辞書の表し方) を使い、既定値も無ければ Left。
-- 誤りの説明の先頭にフィールド名を付ける
field :: FromCapsule a => Map Text Value -> Text -> Maybe Value -> Either Text a
field o name def = case Map.lookup name o of
  Just v -> prefix (fromValue v)
  Nothing -> case def of
    Just d -> prefix (fromValue d)
    Nothing -> Left (name <> ": missing field")
  where
    prefix = either (\msg -> Left (name <> ": " <> msg)) Right

-- | enum を記号の文字列から読む。知らない記号は既定値 (無ければ Left、仕様 §4)
enumFromValue :: Text -> [(Text, a)] -> Maybe a -> Value -> Either Text a
enumFromValue name symbols def v = case v of
  JString s | Just x <- lookup s symbols -> Right x
  JString s -> maybe (Left ("unknown symbol " <> s <> " of " <> name)) Right def
  _ -> Left ("expected " <> name <> " (string), but got " <> T.pack (show v))
