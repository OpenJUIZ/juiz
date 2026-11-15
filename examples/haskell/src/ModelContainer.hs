{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE TemplateHaskell #-}
-- | 型 (Factory) の既定の 3D モデル (docs/spec/container-model.md §5.6、issue #509) を持つテスト用 Container
-- (model_container_hs)。manifest の model のパスはプラグインのファイル (libmodel_container_hs.so など) の
-- あるディレクトリからの相対。テストは juiz_core/tests/integration/container_model_default_test.rs。
module ModelContainer () where

import Juiz

newtype ModelContainer = ModelContainer Int64

manifest :: ContainerManifest
manifest = containerManifest "model_container_hs"
  & description "Container with a default model"
  & model (object
      [ "path" .= JString "models/model_container_hs.stl"
      , "scale" .= JFloat 0.001
      , "color" .= JString "#336699"
      , "origin" .= object ["translation" .= object ["z" .= JFloat (-0.02)]]
      ])

create :: IO ModelContainer
create = pure (ModelContainer 0)

juizContainer 'create 'manifest
