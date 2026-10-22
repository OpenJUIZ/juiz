{-# LANGUAGE TemplateHaskell #-}
-- | Process のエントリポイントを生成する Template Haskell。
--
-- > {-# LANGUAGE TemplateHaskell #-}
-- > increment :: Int64 -> IO Int64
-- > increment arg1 = pure (arg1 + 1)
-- >
-- > juizProcess 'increment 'manifest
--
-- 生成するのは juiz が探す次の C の関数 (bindings/cjuiz/README.md)。
--
-- * @manifest_entry_point@: マニフェストを返す
-- * @process_factory_entry_point@: Process の関数を返す
-- * Process の関数そのもの (@hsjuiz_\<モジュール\>_\<関数\>_process_entry_point@)
module Juiz.TH
  ( juizProcess
  ) where

import Control.Monad (replicateM)
import Data.Char (isAscii, isAlphaNum)
import Foreign.Ptr (FunPtr)
import Language.Haskell.TH

import Juiz.Internal.FFI (ManifestFunction, ProcessFactoryFunction, ProcessFunction)
import Juiz.Process (manifestEntryPoint, processEntryPoint)

-- | @juizProcess 'func 'manifest@: 関数 func を、マニフェスト manifest の Process として公開する。
--
-- func の引数は 'Juiz.Capsule.FromCapsule'、結果は 'Juiz.Capsule.ToCapsule' のインスタンスの型にする。
-- 結果は @IO r@ でも、IO の付かない @r@ でもよい。引数はマニフェストに足した順に渡される。
-- func の型は具体的に書く (多相な関数は使えない)。
juizProcess :: Name -> Name -> Q [Dec]
juizProcess fn man = do
  ty <- reifyType fn >>= expandSynonym
  case ty of
    ForallT {} -> polymorphic
    ForallVisT {} -> polymorphic
    _ -> pure ()
  (argTys, resTy) <- splitFunction ty
  resTy' <- expandSynonym resTy
  let isIO = case resTy' of
        AppT (ConT n) _ -> n == ''IO
        _ -> False
      ioResTy = if isIO then resTy' else AppT (ConT ''IO) resTy'
      fnTy = foldr (\a r -> AppT (AppT ArrowT a) r) ioResTy argTys
  xs <- replicateM (length argTys) (newName "arg")
  let call = foldl appE (varE fn) (map varE xs)
      body = if isIO then call else [| pure $call |]
      lam = if null xs then body else lamE (map varP xs) body
      fnE = sigE lam (pure fnTy)
      base = nameBase fn
      procSym = "hsjuiz_" ++ symbolPart (maybe "" id (nameModule fn)) ++ "_" ++ symbolPart base ++ "_process_entry_point"
      manifestName = mkName ("juizManifestEntryPoint_" ++ base)
      processName = mkName ("juizProcessEntryPoint_" ++ base)
      processPtrName = mkName ("juizProcessEntryPointPtr_" ++ base)
      factoryName = mkName ("juizProcessFactoryEntryPoint_" ++ base)
  sequence
    [ sigD manifestName [t| ManifestFunction |]
    , valD (varP manifestName) (normalB [| manifestEntryPoint $(varE man) $fnE |]) []
    , sigD processName [t| ProcessFunction |]
    , valD (varP processName) (normalB [| processEntryPoint $(varE man) $fnE |]) []
    , forImpD cCall safe ("&" ++ procSym) processPtrName [t| FunPtr ProcessFunction |]
    , sigD factoryName [t| ProcessFactoryFunction |]
    , valD (varP factoryName) (normalB [| pure $(varE processPtrName) |]) []
    , exportD "manifest_entry_point" manifestName [t| ManifestFunction |]
    , exportD procSym processName [t| ProcessFunction |]
    , exportD "process_factory_entry_point" factoryName [t| ProcessFactoryFunction |]
    ]
  where
    exportD sym name qty = ForeignD . ExportF CCall sym name <$> qty
    polymorphic = fail $
      "juizProcess: " ++ show fn ++ " の型が多相です。Int64 や Text など具体的な型を書いてください"

-- | 関数の型を引数の型の並びと結果の型に分ける
splitFunction :: Type -> Q ([Type], Type)
splitFunction t = do
  t' <- expandSynonym t
  case t' of
    AppT (AppT ArrowT a) r -> do
      (as, res) <- splitFunction r
      pure (a : as, res)
    _ -> pure ([], t')

-- | 引数の無い型シノニム (type Handler = Int64 -> IO Int64 など) を展開する
expandSynonym :: Type -> Q Type
expandSynonym t@(ConT n) = do
  info <- reify n
  case info of
    TyConI (TySynD _ [] t') -> expandSynonym t'
    _ -> pure t
expandSynonym (AppT (ConT n) x) = do
  -- type Result = IO のような、結果の型の頭だけのシノニム
  info <- reify n
  case info of
    TyConI (TySynD _ [] t') -> expandSynonym (AppT t' x)
    _ -> pure (AppT (ConT n) x)
expandSynonym t = pure t

-- | C のシンボル名に使えない文字を _ にする
symbolPart :: String -> String
symbolPart = map (\c -> if isAscii c && isAlphaNum c then c else '_')
