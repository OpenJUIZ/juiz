{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE TemplateHaskell #-}
-- | 1 つのライブラリに Process 2 つと Container 1 つ (ContainerProcess 2 つ) をまとめた Component
-- (example_component_hs)。C++ の例 (examples/cpp/component) と同じ構成。
module ExampleComponent () where

import Juiz

import ExampleComponentCounter

component :: ComponentManifest
component = componentManifest "example_component_hs"
  & description "Haskell component example."

incrementManifest :: ProcessManifest
incrementManifest = processManifest "example_component_hs_increment"
  & description "Adds 1 to arg0."
  & addIntArg "arg0" "value to be incremented" 3

increment :: Int64 -> Int64
increment arg0 = arg0 + 1

greetManifest :: ProcessManifest
greetManifest = processManifest "example_component_hs_greet"
  & description "Returns a greeting."
  & addStringArg "name" "name to greet" "juiz"

greet :: Text -> Text
greet name = "Hello, " <> name <> "!"

juizComponent 'component
  [ componentProcess 'increment 'incrementManifest
  , componentProcess 'greet 'greetManifest
  , componentContainerWithDestroy 'createCounter 'destroyCounter 'counterManifest
  , componentContainerProcess 'counterGet 'counterGetManifest
  , componentContainerProcess 'counterIncrement 'counterIncrementManifest
  ]
