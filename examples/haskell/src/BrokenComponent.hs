{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE TemplateHaskell #-}
-- | Component に入っていない Container の ContainerProcess を入れた、誤った Component (broken_component_hs)。
-- juiz が読み込むときにエラーになることを確かめる (juiz のテスト用)。
module BrokenComponent () where

import Juiz

component :: ComponentManifest
component = componentManifest "broken_component_hs"

orphanManifest :: ProcessManifest
orphanManifest = containerProcessManifest "broken_component_hs_orphan" "no_such_container"

orphan :: IORef Int64 -> IO Int64
orphan = readIORef

juizComponent 'component
  [ componentContainerProcess 'orphan 'orphanManifest
  ]
