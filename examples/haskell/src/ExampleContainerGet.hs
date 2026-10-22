{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE TemplateHaskell #-}
-- | example_container_hs の値を返す ContainerProcess (example_container_hs_get)。引数は無い。
module ExampleContainerGet () where

import Juiz

import ExampleContainerState

manifest :: ProcessManifest
manifest = containerProcessManifest "example_container_hs_get" "example_container_hs"
  & description "Returns the value of the container."

get :: ExampleContainer -> IO Int64
get (ExampleContainer ref) = readIORef ref

juizContainerProcess 'get 'manifest
