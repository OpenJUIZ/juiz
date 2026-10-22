{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE TemplateHaskell #-}
-- | 状態の型を example_container_hs と違う型 (IORef Double) で受け取る ContainerProcess (mismatched_container_process_hs)。
-- 型の違う Container に結びつけてもプロセスが落ちずにエラーになることを確かめる、juiz のテスト用。
module MismatchedContainerProcess () where

import Juiz

manifest :: ProcessManifest
manifest = containerProcessManifest "mismatched_container_process_hs" "example_container_hs"
  & description "Expects a container whose state is IORef Double (for tests)."

getDouble :: IORef Double -> IO Double
getDouble = readIORef

juizContainerProcess 'getDouble 'manifest
