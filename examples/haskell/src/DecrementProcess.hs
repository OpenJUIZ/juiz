{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE TemplateHaskell #-}
-- | arg1 から 1 を引く Process (decrement_process_hs)。結果が IO でない関数も公開できる。
module DecrementProcess () where

import Juiz

manifest :: ProcessManifest
manifest = processManifest "decrement_process_hs"
  & description "Subtracts 1 from arg1."
  & addIntArg "arg1" "value to be decremented" 1

decrement :: Int64 -> Int64
decrement arg1 = arg1 - 1

juizProcess 'decrement 'manifest
