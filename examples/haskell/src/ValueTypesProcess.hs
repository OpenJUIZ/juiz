{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE TemplateHaskell #-}
-- | いろいろな型の引数を受け取り、まとめてオブジェクトで返す Process (value_types_process_hs)。
-- hsjuiz の FromCapsule / ToCapsule のテスト (juiz_core/tests/integration/haskell_plugin_test.rs) に使う。
module ValueTypesProcess () where

import qualified Data.Map.Strict as Map
import qualified Data.Text as T
import Juiz

manifest :: ProcessManifest
manifest = processManifest "value_types_process_hs"
  & description "Echoes its arguments as an object."
  & addBoolArg "flag" "a bool" False
  & addIntArg "count" "an int" 0
  & addFloatArg "ratio" "a float" 0.5
  & addStringArg "name" "a string" "juiz"
  & addArrayArg "items" "an array of ints" []
  & addObjectArg "extra" "an object" []

valueTypes :: Bool -> Int64 -> Double -> Text -> [Int64] -> Map Text Value -> IO Value
valueTypes flag count ratio name items extra
  | name == "throw" = error "thrown from Haskell"
  | name == "juiz_error" = throwJuiz (-102) "a JuizError from Haskell"
  | otherwise = pure $ object
      [ "flag" .= not flag
      , "count" .= (count * 2)
      , "ratio" .= (ratio * 2)
      , "name" .= T.toUpper name
      , "items" .= reverse items
      , "sum" .= sum items
      , "extra" .= Map.insert "seen" (JBool True) extra
      , "nothing" .= (Nothing :: Maybe Int64)
      , "nested" .= [object ["k" .= ("v" :: Text)], toValue [JNull, JFloat 1.5]]
      ]

juizProcess 'valueTypes 'manifest
