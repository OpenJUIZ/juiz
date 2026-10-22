{-# LANGUAGE TemplateHaskell #-}
-- | Process / Container / ContainerProcess のエントリポイントを生成する Template Haskell。
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
-- * @process_factory_entry_point@ / @container_factory_entry_point@ / @container_process_factory_entry_point@:
--   Process の関数 / Container を作る関数 / ContainerProcess の関数を返す
-- * @container_factory_destroy_entry_point@: Container を解放する
-- * 関数そのもの (@hsjuiz_\<モジュール\>_\<関数\>_process_entry_point@ など)
module Juiz.TH
  ( juizProcess
  , juizContainer
  , juizContainerWithDestroy
  , juizContainerProcess
  ) where

import Control.Monad (replicateM)
import Data.Char (isAscii, isAlphaNum)
import Foreign.Ptr (FunPtr)
import Language.Haskell.TH

import Juiz.Container
  ( containerCreateEntryPoint
  , containerCreateEntryPointWithDestroy
  , containerDestroyEntryPoint
  , containerManifestEntryPoint
  , containerProcessEntryPoint
  , containerProcessManifestEntryPoint
  )
import Juiz.Internal.FFI
  ( ContainerCreateFunction
  , ContainerDestroyFunction
  , ContainerFactoryFunction
  , ContainerProcessFactoryFunction
  , ContainerProcessFunction
  , ManifestFunction
  , ProcessFactoryFunction
  , ProcessFunction
  )
import Juiz.Process (manifestEntryPoint, processEntryPoint)

-- | @juizProcess 'func 'manifest@: 関数 func を、マニフェスト manifest の Process として公開する。
--
-- func の引数は 'Juiz.Capsule.FromCapsule'、結果は 'Juiz.Capsule.ToCapsule' のインスタンスの型にする。
-- 結果は @IO r@ でも、IO の付かない @r@ でもよい。引数はマニフェストに足した順に渡される。
-- func の型は具体的に書く (多相な関数は使えない)。
juizProcess :: Name -> Name -> Q [Dec]
juizProcess fn man = do
  fnE <- ioFunction "juizProcess" fn
  let procSym = entrySymbol fn "process"
      manifestName = mkName ("juizManifestEntryPoint_" ++ nameBase fn)
      processName = mkName ("juizProcessEntryPoint_" ++ nameBase fn)
      processPtrName = mkName ("juizProcessEntryPointPtr_" ++ nameBase fn)
      factoryName = mkName ("juizProcessFactoryEntryPoint_" ++ nameBase fn)
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

-- | @juizContainer 'create 'manifest@: 関数 create を、マニフェスト manifest ('Juiz.Manifest.containerManifest')
-- の Container を作る関数として公開する。
--
-- create の引数はマニフェストに足した順に、設定ファイルの containers に書いた値 (無ければ既定値) を受け取る。
-- 結果 (@IO s@ または @s@) が Container の状態になり、ContainerProcess の第 1 引数に渡される。
-- 状態を書き換えるときは @IORef@ などを返す。Container を削除すると状態は GC に返される。
juizContainer :: Name -> Name -> Q [Dec]
juizContainer create man = containerDecs create man Nothing

-- | @juizContainerWithDestroy 'create 'destroy 'manifest@: 'juizContainer' に加えて、
-- Container を削除するときに @destroy :: s -> IO ()@ を呼ぶ (デバイスを閉じるなど)
juizContainerWithDestroy :: Name -> Name -> Name -> Q [Dec]
juizContainerWithDestroy create destroy man = containerDecs create man (Just destroy)

containerDecs :: Name -> Name -> Maybe Name -> Q [Dec]
containerDecs create man destroyOpt = do
  fnE <- ioFunction "juizContainer" create
  let createSym = entrySymbol create "container_create"
      base = nameBase create
      manifestName = mkName ("juizContainerManifestEntryPoint_" ++ base)
      createName = mkName ("juizContainerCreateEntryPoint_" ++ base)
      createPtrName = mkName ("juizContainerCreateEntryPointPtr_" ++ base)
      factoryName = mkName ("juizContainerFactoryEntryPoint_" ++ base)
      destroyName = mkName ("juizContainerDestroyEntryPoint_" ++ base)
      createBody = case destroyOpt of
        Nothing -> [| containerCreateEntryPoint $(varE man) $fnE |]
        Just d -> [| containerCreateEntryPointWithDestroy $(varE man) $fnE $(varE d) |]
  sequence
    [ sigD manifestName [t| ManifestFunction |]
    , valD (varP manifestName) (normalB [| containerManifestEntryPoint $(varE man) $fnE |]) []
    , sigD createName [t| ContainerCreateFunction |]
    , valD (varP createName) (normalB createBody) []
    , forImpD cCall safe ("&" ++ createSym) createPtrName [t| FunPtr ContainerCreateFunction |]
    , sigD factoryName [t| ContainerFactoryFunction |]
    , valD (varP factoryName) (normalB [| pure $(varE createPtrName) |]) []
    , sigD destroyName [t| ContainerDestroyFunction |]
    , valD (varP destroyName) (normalB [| containerDestroyEntryPoint $(varE man) |]) []
    , exportD "manifest_entry_point" manifestName [t| ManifestFunction |]
    , exportD createSym createName [t| ContainerCreateFunction |]
    , exportD "container_factory_entry_point" factoryName [t| ContainerFactoryFunction |]
    , exportD "container_factory_destroy_entry_point" destroyName [t| ContainerDestroyFunction |]
    ]

-- | @juizContainerProcess 'func 'manifest@: 関数 func を、マニフェスト manifest
-- ('Juiz.Manifest.containerProcessManifest') の ContainerProcess として公開する。
--
-- func の第 1 引数は Container の状態 (Container を作る関数の結果と同じ型)。続く引数と結果は 'juizProcess' と同じ。
-- 結びつけた Container の状態の型が違うと、呼び出しは 'Juiz.Error.juizHaskellContainerTypeError' のエラーになる。
juizContainerProcess :: Name -> Name -> Q [Dec]
juizContainerProcess fn man = do
  fnE <- ioFunction "juizContainerProcess" fn
  let procSym = entrySymbol fn "container_process"
      base = nameBase fn
      manifestName = mkName ("juizContainerProcessManifestEntryPoint_" ++ base)
      processName = mkName ("juizContainerProcessEntryPoint_" ++ base)
      processPtrName = mkName ("juizContainerProcessEntryPointPtr_" ++ base)
      factoryName = mkName ("juizContainerProcessFactoryEntryPoint_" ++ base)
  sequence
    [ sigD manifestName [t| ManifestFunction |]
    , valD (varP manifestName) (normalB [| containerProcessManifestEntryPoint $(varE man) $fnE |]) []
    , sigD processName [t| ContainerProcessFunction |]
    , valD (varP processName) (normalB [| containerProcessEntryPoint $(varE man) $fnE |]) []
    , forImpD cCall safe ("&" ++ procSym) processPtrName [t| FunPtr ContainerProcessFunction |]
    , sigD factoryName [t| ContainerProcessFactoryFunction |]
    , valD (varP factoryName) (normalB [| pure $(varE processPtrName) |]) []
    , exportD "manifest_entry_point" manifestName [t| ManifestFunction |]
    , exportD procSym processName [t| ContainerProcessFunction |]
    , exportD "container_process_factory_entry_point" factoryName [t| ContainerProcessFactoryFunction |]
    ]

exportD :: String -> Name -> Q Type -> Q Dec
exportD sym name qty = ForeignD . ExportF CCall sym name <$> qty

-- | 関数そのものを公開する C のシンボル名 (hsjuiz_\<モジュール\>_\<関数\>_\<種類\>_entry_point)
entrySymbol :: Name -> String -> String
entrySymbol fn kind =
  "hsjuiz_" ++ symbolPart (maybe "" id (nameModule fn)) ++ "_" ++ symbolPart (nameBase fn) ++ "_" ++ kind ++ "_entry_point"

-- | 関数 fn を、結果が IO の関数の式 (型注釈付き) にする。結果が IO でなければ pure で包む
ioFunction :: String -> Name -> Q ExpQ
ioFunction what fn = do
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
  pure (sigE lam (pure fnTy))
  where
    polymorphic = fail $
      what ++ ": " ++ show fn ++ " の型が多相です。Int64 や Text など具体的な型を書いてください"

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
