{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE TemplateHaskell #-}
-- | arg1 に 1 を足す Process (increment_process_hs)。
module IncrementProcess () where

import Juiz

manifest :: ProcessManifest
manifest = processManifest "increment_process_hs"
  & description "Adds 1 to arg1."
  & addIntArg "arg1" "value to be incremented" 1

increment :: Int64 -> IO Int64
increment arg1 = pure (arg1 + 1)

juizProcess 'increment 'manifest
