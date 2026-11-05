-- | juiz のエラーコード (bindings/cjuiz/include/juiz_sdk.h と同じ値) と、Haskell 側で投げる例外。
module Juiz.Error
  ( JuizError (..)
  , throwJuiz
    -- * エラーコード
  , juizOk
  , juizValueTypeError
  , juizValueNotFoundError
  , juizValueConverterError
  , juizCapsulePtrLockError
  , juizCapsuleMapNoValue
  , juizCapsuleTypeError
  , juizCapsuleNoValue
  , juizProcessFunctionError
  , juizHaskellContainerTypeError
  , juizTypeSchemaInvalidError
  , juizValueDoesNotMatchTypeError
  , juizImageFormatError
  , juizTransformationsNotAttachedError
  , juizNoCommonAnchorError
  , juizPoseNotWritableError
  , juizInvalidPoseValueError
  , juizTransformationsError
  , juizTransformNotWritableError
  , juizTransformNotFoundError
  ) where

import Control.Exception (Exception, throwIO)
import Data.Int (Int64)
import Data.Text (Text)

-- | juiz のエラーコードと説明。プラグインの関数の中で投げると、エントリポイントがエラーコードに変えて juiz に返す
data JuizError = JuizError
  { juizErrorCode :: !Int64
  , juizErrorMessage :: !Text
  }
  deriving (Show)

instance Exception JuizError

throwJuiz :: Int64 -> Text -> IO a
throwJuiz code msg = throwIO (JuizError code msg)

juizOk, juizValueTypeError, juizValueNotFoundError, juizValueConverterError :: Int64
juizOk = 0
juizValueTypeError = -101
juizValueNotFoundError = -102
juizValueConverterError = -103

juizCapsulePtrLockError, juizCapsuleMapNoValue, juizCapsuleTypeError, juizCapsuleNoValue :: Int64
juizCapsulePtrLockError = -201
juizCapsuleMapNoValue = -301
juizCapsuleTypeError = -401
juizCapsuleNoValue = -402

-- | Process の関数が値を返せなかった (juiz_sdk.h の JUIZ_PROCESS_FUNCTION_NULL_OPT_RETURNED)。
-- 'JuizError' 以外の例外 (error や自分で投げた例外) はこのコードになる
juizProcessFunctionError :: Int64
juizProcessFunctionError = -501

-- | ContainerProcess に、期待する型ではない Container が渡された (juiz_sdk.h の JUIZ_HASKELL_CONTAINER_TYPE_ERROR)。
-- 別の言語の Container や、状態の型が違う Haskell の Container に結びつけたとき
juizHaskellContainerTypeError :: Int64
juizHaskellContainerTypeError = -602

-- | Avro のスキーマが juiz のデータ型の規則に合わない (juiz_sdk.h の JUIZ_TYPE_SCHEMA_INVALID_ERROR)
juizTypeSchemaInvalidError :: Int64
juizTypeSchemaInvalidError = -701

-- | 辞書の値が型に合わない (juiz_sdk.h の JUIZ_VALUE_DOES_NOT_MATCH_TYPE_ERROR)。
-- 型付きの引数に合わない値が渡されたとき (理由は juiz のログに出る)
juizValueDoesNotMatchTypeError :: Int64
juizValueDoesNotMatchTypeError = -702

-- | 画像の形式・大きさ・stride が正しくない、または扱えない形式 (juiz_sdk.h の JUIZ_IMAGE_FORMAT_ERROR)
juizImageFormatError :: Int64
juizImageFormatError = -704

-- | ContainerPose (docs/spec/transformations.md §12.3・§17)。
-- Container が System の Transformations につながっていない、または ContainerProcess の外で読み書きした
juizTransformationsNotAttachedError :: Int64
juizTransformationsNotAttachedError = -901

-- | 基準と Container の両方を含む Anchor がない
juizNoCommonAnchorError :: Int64
juizNoCommonAnchorError = -902

-- | 根の Anchor・基準自身・基準が子孫の Container への書き込み
juizPoseNotWritableError :: Int64
juizPoseNotWritableError = -903

-- | 長さ 0 の四元数、NaN・無限大
juizInvalidPoseValueError :: Int64
juizInvalidPoseValueError = -904

-- | そのほかの ContainerPose の失敗
juizTransformationsError :: Int64
juizTransformationsError = -905

-- | 'Juiz.Pose.setTransform' で、呼び出し元 Container が端点でも writers でもない Transform への書き込み、
-- readonly の ContainerProcess からの書き込み (§7.5、#410)
juizTransformNotWritableError :: Int64
juizTransformNotWritableError = -906

-- | 'Juiz.Pose.setTransform' の Transform が無い (§7.5、#410)
juizTransformNotFoundError :: Int64
juizTransformNotFoundError = -907
