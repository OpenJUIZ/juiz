{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE TemplateHaskell #-}
-- | 誤った型付きの Process (broken_typed_process_hs)。関数は Pose2D を受け取るのに、マニフェストでは
-- 型なしの Object の引数にしている。juiz が読み込むときにエラーになる (juiz のテスト用)
module BrokenTypedProcess () where

import Juiz

import Robot.Geometry

manifest :: ProcessManifest
manifest = processManifest "broken_typed_process_hs"
  & addObjectArg "pose" "" []

poseTheta :: Pose2D -> Double
poseTheta = pose2DTheta

juizProcess 'poseTheta 'manifest
