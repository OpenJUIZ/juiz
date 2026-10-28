{-# LANGUAGE TemplateHaskell #-}
-- | Process / Container / ContainerProcess / Component のエントリポイントを生成する Template Haskell。
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
--
-- Component ('juizComponent') では、上の factory の関数の名前が @\<関数名\>_factory_entry_point@ などになり、
-- マニフェストは @component_manifest_entry_point@ が返す。
module Juiz.TH
  ( juizProcess
  , juizContainer
  , juizContainerWithDestroy
  , juizContainerProcess
  , juizComponent
  , ComponentEntry
  , componentProcess
  , componentContainer
  , componentContainerWithDestroy
  , componentContainerProcess
  ) where

import Control.Monad (replicateM)
import qualified Data.Text as T
import Data.Char (isAscii, isAlphaNum)
import Foreign.Ptr (FunPtr)
import Language.Haskell.TH

import Juiz.Component (componentManifestEntryPoint, containerPart, containerProcessPart, processPart)
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
import Juiz.Manifest (ContainerManifest, ProcessManifest, factory)
import Juiz.Process (manifestEntryPoint, processEntryPoint)

-- | @juizProcess 'func 'manifest@: 関数 func を、マニフェスト manifest の Process として公開する。
--
-- func の引数は 'Juiz.Capsule.FromCapsule'、結果は 'Juiz.Capsule.ToCapsule' のインスタンスの型にする。
-- 結果は @IO r@ でも、IO の付かない @r@ でもよい。引数はマニフェストに足した順に渡される。
-- func の型は具体的に書く (多相な関数は使えない)。
juizProcess :: Name -> Name -> Q [Dec]
juizProcess fn man = do
  fnE <- ioFunction "juizProcess" fn
  processDecs fn fnE (varE man) "manifest_entry_point" "process_factory"

-- | Process の関数・マニフェスト・関数を返す関数を生成する。factorySym はマニフェストの factory
processDecs :: Name -> ExpQ -> ExpQ -> String -> String -> Q [Dec]
processDecs fn fnE manE manifestSym factorySym = do
  let procSym = entrySymbol fn "process"
      manifestName = mkName ("juizManifestEntryPoint_" ++ nameBase fn)
      processName = mkName ("juizProcessEntryPoint_" ++ nameBase fn)
      processPtrName = mkName ("juizProcessEntryPointPtr_" ++ nameBase fn)
      factoryName = mkName ("juizProcessFactoryEntryPoint_" ++ nameBase fn)
  sequence
    [ sigD manifestName [t| ManifestFunction |]
    , valD (varP manifestName) (normalB [| manifestEntryPoint $manE $fnE |]) []
    , sigD processName [t| ProcessFunction |]
    , valD (varP processName) (normalB [| processEntryPoint $manE $fnE |]) []
    , forImpD cCall safe ("&" ++ procSym) processPtrName [t| FunPtr ProcessFunction |]
    , sigD factoryName [t| ProcessFactoryFunction |]
    , valD (varP factoryName) (normalB [| pure $(varE processPtrName) |]) []
    , exportD manifestSym manifestName [t| ManifestFunction |]
    , exportD procSym processName [t| ProcessFunction |]
    , exportD (factorySym ++ "_entry_point") factoryName [t| ProcessFactoryFunction |]
    ]

-- | @juizContainer 'create 'manifest@: 関数 create を、マニフェスト manifest ('Juiz.Manifest.containerManifest')
-- の Container を作る関数として公開する。
--
-- create の引数はマニフェストに足した順に、設定ファイルの containers に書いた値 (無ければ既定値) を受け取る。
-- 結果 (@IO s@ または @s@) が Container の状態になり、ContainerProcess の第 1 引数に渡される。
-- 状態を書き換えるときは @IORef@ などを返す。Container を削除すると状態は GC に返される。
juizContainer :: Name -> Name -> Q [Dec]
juizContainer create man = do
  fnE <- ioFunction "juizContainer" create
  containerDecs create fnE (varE man) Nothing "manifest_entry_point" "container_factory"

-- | @juizContainerWithDestroy 'create 'destroy 'manifest@: 'juizContainer' に加えて、
-- Container を削除するときに @destroy :: s -> IO ()@ を呼ぶ (デバイスを閉じるなど)
juizContainerWithDestroy :: Name -> Name -> Name -> Q [Dec]
juizContainerWithDestroy create destroy man = do
  fnE <- ioFunction "juizContainerWithDestroy" create
  containerDecs create fnE (varE man) (Just destroy) "manifest_entry_point" "container_factory"

-- | Container を作る関数・解放する関数・マニフェスト・作る関数を返す関数を生成する。factorySym はマニフェストの factory
containerDecs :: Name -> ExpQ -> ExpQ -> Maybe Name -> String -> String -> Q [Dec]
containerDecs create fnE manE destroyOpt manifestSym factorySym = do
  let createSym = entrySymbol create "container_create"
      base = nameBase create
      manifestName = mkName ("juizContainerManifestEntryPoint_" ++ base)
      createName = mkName ("juizContainerCreateEntryPoint_" ++ base)
      createPtrName = mkName ("juizContainerCreateEntryPointPtr_" ++ base)
      factoryName = mkName ("juizContainerFactoryEntryPoint_" ++ base)
      destroyName = mkName ("juizContainerDestroyEntryPoint_" ++ base)
      createBody = case destroyOpt of
        Nothing -> [| containerCreateEntryPoint $manE $fnE |]
        Just d -> [| containerCreateEntryPointWithDestroy $manE $fnE $(varE d) |]
  sequence
    [ sigD manifestName [t| ManifestFunction |]
    , valD (varP manifestName) (normalB [| containerManifestEntryPoint $manE $fnE |]) []
    , sigD createName [t| ContainerCreateFunction |]
    , valD (varP createName) (normalB createBody) []
    , forImpD cCall safe ("&" ++ createSym) createPtrName [t| FunPtr ContainerCreateFunction |]
    , sigD factoryName [t| ContainerFactoryFunction |]
    , valD (varP factoryName) (normalB [| pure $(varE createPtrName) |]) []
    , sigD destroyName [t| ContainerDestroyFunction |]
    , valD (varP destroyName) (normalB [| containerDestroyEntryPoint $manE |]) []
    , exportD manifestSym manifestName [t| ManifestFunction |]
    , exportD createSym createName [t| ContainerCreateFunction |]
    , exportD (factorySym ++ "_entry_point") factoryName [t| ContainerFactoryFunction |]
    , exportD (factorySym ++ "_destroy_entry_point") destroyName [t| ContainerDestroyFunction |]
    ]

-- | @juizContainerProcess 'func 'manifest@: 関数 func を、マニフェスト manifest
-- ('Juiz.Manifest.containerProcessManifest') の ContainerProcess として公開する。
--
-- func の第 1 引数は Container の状態 (Container を作る関数の結果と同じ型)。続く引数と結果は 'juizProcess' と同じ。
-- 結びつけた Container の状態の型が違うと、呼び出しは 'Juiz.Error.juizHaskellContainerTypeError' のエラーになる。
juizContainerProcess :: Name -> Name -> Q [Dec]
juizContainerProcess fn man = do
  fnE <- ioFunction "juizContainerProcess" fn
  containerProcessDecs fn fnE (varE man) "manifest_entry_point" "container_process_factory"

-- | ContainerProcess の関数・マニフェスト・関数を返す関数を生成する。factorySym はマニフェストの factory
containerProcessDecs :: Name -> ExpQ -> ExpQ -> String -> String -> Q [Dec]
containerProcessDecs fn fnE manE manifestSym factorySym = do
  let procSym = entrySymbol fn "container_process"
      base = nameBase fn
      manifestName = mkName ("juizContainerProcessManifestEntryPoint_" ++ base)
      processName = mkName ("juizContainerProcessEntryPoint_" ++ base)
      processPtrName = mkName ("juizContainerProcessEntryPointPtr_" ++ base)
      factoryName = mkName ("juizContainerProcessFactoryEntryPoint_" ++ base)
  sequence
    [ sigD manifestName [t| ManifestFunction |]
    , valD (varP manifestName) (normalB [| containerProcessManifestEntryPoint $manE $fnE |]) []
    , sigD processName [t| ContainerProcessFunction |]
    , valD (varP processName) (normalB [| containerProcessEntryPoint $manE $fnE |]) []
    , forImpD cCall safe ("&" ++ procSym) processPtrName [t| FunPtr ContainerProcessFunction |]
    , sigD factoryName [t| ContainerProcessFactoryFunction |]
    , valD (varP factoryName) (normalB [| pure $(varE processPtrName) |]) []
    , exportD manifestSym manifestName [t| ManifestFunction |]
    , exportD procSym processName [t| ContainerProcessFunction |]
    , exportD (factorySym ++ "_entry_point") factoryName [t| ContainerProcessFactoryFunction |]
    ]

-- | 'juizComponent' に渡す、Component の要素
data ComponentEntry
  = EntryProcess Name Name
  | EntryContainer Name Name (Maybe Name)
  | EntryContainerProcess Name Name

-- | @componentProcess 'func 'manifest@: 'juizProcess' と同じ関数とマニフェストの Process
componentProcess :: Name -> Name -> ComponentEntry
componentProcess = EntryProcess

-- | @componentContainer 'create 'manifest@: 'juizContainer' と同じ関数とマニフェストの Container
componentContainer :: Name -> Name -> ComponentEntry
componentContainer create man = EntryContainer create man Nothing

-- | @componentContainerWithDestroy 'create 'destroy 'manifest@: 'juizContainerWithDestroy' と同じ Container
componentContainerWithDestroy :: Name -> Name -> Name -> ComponentEntry
componentContainerWithDestroy create destroy man = EntryContainer create man (Just destroy)

-- | @componentContainerProcess 'func 'manifest@: 'juizContainerProcess' と同じ関数とマニフェストの ContainerProcess
componentContainerProcess :: Name -> Name -> ComponentEntry
componentContainerProcess = EntryContainerProcess

-- | @juizComponent 'manifest [componentProcess 'f 'fManifest, componentContainer 'create 'cManifest, ...]@:
-- 1 つのライブラリに複数の Process / Container / ContainerProcess をまとめた Component として公開する。
--
-- manifest は 'Juiz.Manifest.componentManifest' で作り、名前を設定ファイルの components に書く。
-- 要素のマニフェストは単体のときと同じものでよく、factory (シンボル名) は関数名から付け直す。
-- 生成する C の関数は C++ の COMPONENT_PROCESS_FACTORY などと同じ名前
-- (@component_manifest_entry_point@、@\<関数名\>_factory_entry_point@、@\<関数名\>_manifest_entry_point@ など)。
-- 関数名は Component の中で重ならないようにする (別のモジュールの関数も使える)。
-- 単体の 'juizProcess' などと同じライブラリに入れてもよいが、その関数を Component にも入れることはできない。
juizComponent :: Name -> [ComponentEntry] -> Q [Dec]
juizComponent comp entries = do
  let bases = map (symbolPart . nameBase . entryFunction) entries
  case [b | (i, b) <- zip [0 :: Int ..] bases, b `elem` take i bases] of
    (b : _) -> fail $ "juizComponent: 関数名 " ++ b ++ " が Component の中で重なっています"
    [] -> pure ()
  results <- mapM componentEntryDecs entries
  let manifestName = mkName "juizComponentManifestEntryPoint"
  top <- sequence
    [ sigD manifestName [t| ManifestFunction |]
    , valD (varP manifestName)
        (normalB [| componentManifestEntryPoint $(varE comp) $(listE (map snd results)) |]) []
    , exportD "component_manifest_entry_point" manifestName [t| ManifestFunction |]
    ]
  pure (concatMap fst results ++ top)

entryFunction :: ComponentEntry -> Name
entryFunction (EntryProcess fn _) = fn
entryFunction (EntryContainer fn _ _) = fn
entryFunction (EntryContainerProcess fn _) = fn

-- | 要素のエントリポイントと、component_manifest_entry_point に渡す 'Juiz.Component.ComponentPart' の式
componentEntryDecs :: ComponentEntry -> Q ([Dec], ExpQ)
componentEntryDecs entry = do
  let fn = entryFunction entry
      base = symbolPart (nameBase fn)
      factorySym = base ++ "_factory"
      manifestSym = base ++ "_manifest_entry_point"
      manName = mkName ("juizComponentManifest_" ++ nameBase fn)
      manE = varE manName
      manDecs ty man = sequence
        [ sigD manName ty
        , valD (varP manName) (normalB [| factory (T.pack $(litE (stringL factorySym))) $(varE man) |]) []
        ]
  case entry of
    EntryProcess _ man -> do
      fnE <- ioFunction "componentProcess" fn
      ds <- (++) <$> manDecs [t| ProcessManifest |] man <*> processDecs fn fnE manE manifestSym factorySym
      pure (ds, [| processPart $manE $fnE |])
    EntryContainer _ man destroyOpt -> do
      fnE <- ioFunction "componentContainer" fn
      ds <- (++) <$> manDecs [t| ContainerManifest |] man
                 <*> containerDecs fn fnE manE destroyOpt manifestSym factorySym
      pure (ds, [| containerPart $manE $fnE |])
    EntryContainerProcess _ man -> do
      fnE <- ioFunction "componentContainerProcess" fn
      ds <- (++) <$> manDecs [t| ProcessManifest |] man
                 <*> containerProcessDecs fn fnE manE manifestSym factorySym
      pure (ds, [| containerProcessPart $manE $fnE |])

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
