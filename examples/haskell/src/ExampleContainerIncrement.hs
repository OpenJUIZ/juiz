{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE TemplateHaskell #-}
-- | example_container_hs の値に arg0 を足し、足した後の値を返す ContainerProcess (example_container_hs_increment)。
module ExampleContainerIncrement () where

import Juiz

import ExampleContainerState

manifest :: ProcessManifest
manifest = containerProcessManifest "example_container_hs_increment" "example_container_hs"
  & description "Adds arg0 to the value of the container."
  & addIntArg "arg0" "value to be added" 1

-- | 第 1 引数が Container の状態
increment :: ExampleContainer -> Int64 -> IO Int64
increment (ExampleContainer ref) arg0 = atomicModifyIORef' ref (\v -> (v + arg0, v + arg0))

juizContainerProcess 'increment 'manifest
