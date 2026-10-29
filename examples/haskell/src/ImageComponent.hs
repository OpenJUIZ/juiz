{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE TemplateHaskell #-}
-- | 画像の引数・戻り値の例とテスト用 Component (image_component_hs、juiz_core/tests/integration/plugin_image_test.rs、
-- issue #253、docs/spec/large-data.md §10)。C++ の cpp_image_component と同じ構成。
module ImageComponent () where

import qualified Data.ByteString as BS
import qualified Data.Text as T
import Data.Word (Word8)
import Juiz

component :: ComponentManifest
component = componentManifest "image_component_hs"
  & description "Haskell image component (test)."

-- 8 bit の形式は画素を反転した画像、それ以外は同じ画像を返す
invertManifest :: ProcessManifest
invertManifest = processManifest "image_invert_hs"
  & addImageArg "img" "image to invert"

invert :: Image -> Image
invert img
  | imageFormat img `elem` [L8, LA8, RGB8, RGBA8] = img { imageData = BS.map (255 -) (imageData img) }
  | otherwise = img

-- 画素 (x, y) が B = x, G = y, R = 7 の BGR8 の画像を返す (juiz には RGB8 で入る)
bgrManifest :: ProcessManifest
bgrManifest = processManifest "image_bgr_hs"
  & addIntArg "width" "" 4
  & addIntArg "height" "" 3

bgr :: Int64 -> Int64 -> IO Image
bgr w h = either (throwJuiz juizImageFormatError) pure $
  newImage (fromIntegral w) (fromIntegral h) BGR8 $ BS.pack
    [ c | y <- [0 .. h - 1], x <- [0 .. w - 1], c <- [fromIntegral x, fromIntegral y, 7 :: Word8] ]

-- 画像の大きさと形式と、画像の後ろの引数を文字列にする
infoManifest :: ProcessManifest
infoManifest = processManifest "image_info_hs"
  & addImageArg "img" ""
  & addStringArg "label" "" ""

info :: Image -> Text -> Text
info img label = T.concat
  [ label, ":", tshow (imageWidth img), "x", tshow (imageHeight img), ":", formatName (imageFormat img) ]
  where tshow :: Show a => a -> Text
        tshow = T.pack . show

-- 形式名 (la16 など)。Windows の DLL では構成子が 8 個以上の型の show や case が落ちるので (issue #262)、
-- fromEnum で表を引く
formatName :: PixelFormat -> Text
formatName f = names !! fromEnum f
  where names = ["l8", "la8", "rgb8", "rgba8", "l16", "la16", "rgb16", "rgba16", "rgb32f", "rgba32f", "bgr8", "bgra8"]

-- 最後に受け取った画像を持つ Container
storeManifest :: ContainerManifest
storeManifest = containerManifest "image_store_hs"

createStore :: IO (IORef (Int64, Maybe Image))
createStore = newIORef (0, Nothing)

storePutManifest :: ProcessManifest
storePutManifest = containerProcessManifest "image_store_hs_put" "image_store_hs"
  & addImageArg "img" ""

storePut :: IORef (Int64, Maybe Image) -> Image -> IO Int64
storePut ref img = atomicModifyIORef' ref (\(n, _) -> ((n + 1, Just img), n + 1))

storeGetManifest :: ProcessManifest
storeGetManifest = containerProcessManifest "image_store_hs_get" "image_store_hs"

storeGet :: IORef (Int64, Maybe Image) -> IO Image
storeGet ref = readIORef ref >>= maybe (throwJuiz juizImageFormatError "no image yet") pure . snd

juizComponent 'component
  [ componentProcess 'invert 'invertManifest
  , componentProcess 'bgr 'bgrManifest
  , componentProcess 'info 'infoManifest
  , componentContainer 'createStore 'storeManifest
  , componentContainerProcess 'storePut 'storePutManifest
  , componentContainerProcess 'storeGet 'storeGetManifest
  ]
