{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE TemplateHaskell #-}
-- | 整数を一つ保持する Container (example_container_hs)。
-- 初期値は設定ファイルの containers に書いた value (無ければ 0)。
--
-- Container を削除すると destroy が呼ばれる。juiz のテスト (juiz_core/tests/integration/haskell_container_test.rs) が
-- 生きている Container の数を数えられるよう、example_container_hs_alive_count も公開している。
module ExampleContainer () where

import Juiz
import System.IO.Unsafe (unsafePerformIO)

import ExampleContainerState

manifest :: ContainerManifest
manifest = containerManifest "example_container_hs"
  & description "Holds an integer value."
  & addIntArg "value" "initial value" 0

create :: Int64 -> IO ExampleContainer
create value = do
  _ <- atomicModifyIORef' aliveCount (\n -> (n + 1, ()))
  ExampleContainer <$> newIORef value

destroy :: ExampleContainer -> IO ()
destroy _ = atomicModifyIORef' aliveCount (\n -> (n - 1, ()))

-- | 作られて、まだ削除されていない Container の数 (テスト用)
aliveCount :: IORef Int64
aliveCount = unsafePerformIO (newIORef 0)
{-# NOINLINE aliveCount #-}

getAliveCount :: IO Int64
getAliveCount = readIORef aliveCount

foreign export ccall "example_container_hs_alive_count" getAliveCount :: IO Int64

juizContainerWithDestroy 'create 'destroy 'manifest
