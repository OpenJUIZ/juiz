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
