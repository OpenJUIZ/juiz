{-# LANGUAGE OverloadedStrings #-}
-- | example_component_hs に入れる Container (example_component_hs_counter) と、その ContainerProcess。
--
-- Component の要素は別のモジュールに書いてもよい。関数とマニフェストを公開し、
-- ExampleComponent.hs の juizComponent に渡す。
-- Container を削除すると destroy が呼ばれ、juiz のテスト (juiz_core/tests/integration/haskell_component_test.rs) が
-- 数えられるよう、生きている Container の数を example_component_hs_alive_count で公開している。
module ExampleComponentCounter
  ( Counter
  , counterManifest
  , createCounter
  , destroyCounter
  , counterGetManifest
  , counterGet
  , counterIncrementManifest
  , counterIncrement
  ) where

import Juiz
import System.IO.Unsafe (unsafePerformIO)

-- | Container の状態。Component では Container と ContainerProcess が同じライブラリに入るので、
-- 状態の型を別のモジュールに分けなくてよい
newtype Counter = Counter (IORef Int64)

counterManifest :: ContainerManifest
counterManifest = containerManifest "example_component_hs_counter"
  & description "Holds an integer value."
  & addIntArg "value" "initial value" 0

createCounter :: Int64 -> IO Counter
createCounter value = do
  _ <- atomicModifyIORef' aliveCount (\n -> (n + 1, ()))
  Counter <$> newIORef value

destroyCounter :: Counter -> IO ()
destroyCounter _ = atomicModifyIORef' aliveCount (\n -> (n - 1, ()))

counterGetManifest :: ProcessManifest
counterGetManifest = containerProcessManifest "example_component_hs_counter_get" "example_component_hs_counter"
  & description "Returns the value of the counter."

counterGet :: Counter -> IO Int64
counterGet (Counter ref) = readIORef ref

counterIncrementManifest :: ProcessManifest
counterIncrementManifest =
  containerProcessManifest "example_component_hs_counter_increment" "example_component_hs_counter"
    & description "Adds arg0 to the value of the counter."
    & addIntArg "arg0" "value to be added" 1

counterIncrement :: Counter -> Int64 -> IO Int64
counterIncrement (Counter ref) arg0 = atomicModifyIORef' ref (\v -> (v + arg0, v + arg0))

-- | 作られて、まだ削除されていない Container の数 (テスト用)
aliveCount :: IORef Int64
aliveCount = unsafePerformIO (newIORef 0)
{-# NOINLINE aliveCount #-}

getAliveCount :: IO Int64
getAliveCount = readIORef aliveCount

foreign export ccall "example_component_hs_alive_count" getAliveCount :: IO Int64
