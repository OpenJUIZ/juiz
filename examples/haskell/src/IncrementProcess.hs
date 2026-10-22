{-# LANGUAGE ForeignFunctionInterface #-}
-- | 手書きの FFI だけで書いた最小の Haskell Process (increment_process_hs)。
-- juiz は increment_process_hs の共有ライブラリを開き、juiz_hs_init → manifest_entry_point →
-- process_factory_entry_point の順に呼ぶ (bindings/cjuiz/README.md)。
module IncrementProcess () where

import Data.Int (Int64)
import Foreign
import JuizFFI

increment :: Int64 -> Int64
increment arg1 = arg1 + 1

manifestEntryPoint :: Ptr CapsulePtr -> IO Int64
manifestEntryPoint cp = guardException $ writeManifest cp $ \v -> do
  setString v "type_name" "increment_process_hs"
  setString v "language" "haskell"
  setString v "description" "Adds 1 to arg1."
  setString v "factory" "process_factory"
  args <- setEmptyArray v "arguments"
  a <- pushObject args
  setString a "name" "arg1"
  setString a "type_name" "Int"
  setString a "description" "test_argument"
  setInt a "default" 1

processEntryPoint :: ProcessFunction
processEntryPoint cm out = guardException $ do
  arg1 <- getIntArg cm "arg1"
  case arg1 of
    Left err -> pure err
    Right v -> setIntResult out (increment v)

processFactoryEntryPoint :: IO (FunPtr ProcessFunction)
processFactoryEntryPoint = pure p_processEntryPoint

foreign export ccall "manifest_entry_point" manifestEntryPoint :: Ptr CapsulePtr -> IO Int64
foreign export ccall "increment_process_hs_process_entry_point" processEntryPoint :: ProcessFunction
foreign import ccall "&increment_process_hs_process_entry_point" p_processEntryPoint :: FunPtr ProcessFunction
foreign export ccall "process_factory_entry_point" processFactoryEntryPoint :: IO (FunPtr ProcessFunction)
