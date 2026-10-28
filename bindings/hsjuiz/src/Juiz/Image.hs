{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE ScopedTypeVariables #-}
-- | 画像の引数と戻り値 (issue #253、docs/spec/large-data.md §10)。
--
-- Process の引数を 'Image' にすると、届いた画像 (Rust の Capsule::Image) を複製して受け取る。
-- 結果を 'Image' にすると、juiz の画像に複製して返す。IPC や qmp のブローカー越しには生バイトのフレームで渡る。
-- マニフェストでは 'Juiz.Manifest.addImageArg' で引数を足す。
--
-- > manifest :: ProcessManifest
-- > manifest = processManifest "invert_hs" & addImageArg "img" "image to invert"
-- >
-- > invert :: Image -> IO Image
-- > invert img = pure img { imageData = BS.map (255 -) (imageData img) }
module Juiz.Image
  ( Image (..)
  , PixelFormat (..)
  , bytesPerPixel
  , newImage
  , imageRowBytes
  , readImage
  , writeImage
  ) where

import Control.Exception (SomeException, bracket, throwIO, try)
import Control.Monad (unless, when)
import qualified Data.ByteString as BS
import qualified Data.ByteString.Unsafe as BSU
import Data.Int (Int64)
import Data.IORef
import Data.Text (Text)
import qualified Data.Text as T
import Data.Word (Word32)
import Foreign.C.Types (CSize)
import Foreign.Marshal.Alloc (allocaBytes)
import Foreign.Ptr (Ptr, castPtr, plusPtr)
import Foreign.StablePtr (castStablePtrToPtr, freeStablePtr, newStablePtr)
import Foreign.Storable (peekByteOff, pokeByteOff, sizeOf)

import Juiz.Error
import Juiz.Internal.FFI

-- | 画素の形式。'L8' 〜 'RGBA32F' は large-data.md §3.4 の表と同じ。
-- 'BGR8' / 'BGRA8' は送るとき (Process の結果) だけ使え、juiz は赤と青を入れ替えて 'RGB8' / 'RGBA8' で持つ
data PixelFormat
  = L8 | LA8 | RGB8 | RGBA8 | L16 | LA16 | RGB16 | RGBA16 | RGB32F | RGBA32F | BGR8 | BGRA8
  deriving (Eq, Ord, Show, Enum, Bounded)

-- | 画像。画素は 'imageData' の先頭から 1 行 'imageStride' バイトずつ並ぶ。16 bit・f32 の形式はホストのバイト順。
-- 受け取った画像の 'imageStride' は 'imageRowBytes' (行の余白なし)
data Image = Image
  { imageWidth :: !Int
  , imageHeight :: !Int
  , imageFormat :: !PixelFormat
  , imageStride :: !Int
  , imageData :: !BS.ByteString
  }
  deriving (Eq, Show)

-- | 1 画素のバイト数
bytesPerPixel :: PixelFormat -> Int
bytesPerPixel f = case f of
  L8 -> 1
  LA8 -> 2
  L16 -> 2
  RGB8 -> 3
  BGR8 -> 3
  RGBA8 -> 4
  BGRA8 -> 4
  LA16 -> 4
  RGB16 -> 6
  RGBA16 -> 8
  RGB32F -> 12
  RGBA32F -> 16

-- | 1 行の画素のバイト数 (幅 × 1 画素のバイト数)
imageRowBytes :: Image -> Int
imageRowBytes img = imageWidth img * bytesPerPixel (imageFormat img)

-- | 行の余白のない画像を作る。画素のバイト数が 幅 × 高さ × 1 画素のバイト数 でなければ 'Left'
newImage :: Int -> Int -> PixelFormat -> BS.ByteString -> Either Text Image
newImage w h f bytes
  | w < 0 || h < 0 = Left "width and height must not be negative"
  | BS.length bytes /= w * h * bytesPerPixel f =
      Left ("image data must be " <> T.pack (show (w * h * bytesPerPixel f)) <> " bytes, but "
        <> T.pack (show (BS.length bytes)))
  | otherwise = Right (Image w h f (w * bytesPerPixel f) bytes)

-- juiz_sdk.h の juiz_image: uint32_t width, height, format, reserved; size_t stride; const uint8_t* data
offStride, offData, imageStructSize :: Int
offStride = 16
offData = offStride + sizeOf (undefined :: CSize)
imageStructSize = offData + sizeOf (undefined :: Ptr ())

formatCode :: PixelFormat -> Word32
formatCode = fromIntegral . fromEnum

-- | capsule_ptr の画像を読む (複製する)。画像でなければ 'juizCapsuleTypeError' の 'JuizError'
readImage :: Ptr CCapsulePtr -> IO Image
readImage cp = do
  result <- newIORef Nothing
  failure <- newIORef Nothing
  let callback p = do
        r <- try $ do
          w <- peekByteOff p 0 :: IO Word32
          h <- peekByteOff p 4 :: IO Word32
          code <- peekByteOff p 8 :: IO Word32
          stride <- peekByteOff p offStride :: IO CSize
          dat <- peekByteOff p offData :: IO (Ptr ())
          when (fromIntegral code > fromEnum (maxBound :: PixelFormat)) $
            throwJuiz juizImageFormatError ("unknown image format " <> T.pack (show code))
          let f = toEnum (fromIntegral code)
              rb = fromIntegral w * bytesPerPixel f
              st = fromIntegral stride
          rows <- mapM (\y -> BS.packCStringLen (castPtr dat `plusPtr` (y * st), rb)) [0 .. fromIntegral h - 1]
          writeIORef result (Just (Image (fromIntegral w) (fromIntegral h) f rb (BS.concat rows)))
        case r of
          Left (e :: SomeException) -> writeIORef failure (Just e) >> pure juizValueConverterError
          Right () -> pure juizOk
  code <- bracket (newStablePtr (callback :: ImageFunction)) freeStablePtr
    (c_capsule_ptr_lock_as_image cp imageTrampoline . castStablePtrToPtr)
  readIORef failure >>= maybe (pure ()) throwIO
  unless (code == juizOk) $ throwJuiz code (codeMessage code)
  maybe (throwJuiz juizCapsuleTypeError "not an image") pure =<< readIORef result
  where
    codeMessage :: Int64 -> Text
    codeMessage c
      | c == juizImageFormatError = "unsupported image format"
      | otherwise = "not an image"

-- | 画像を capsule (Process の戻り値) に書き込む (libjuiz_sdk が複製する)。戻り値は juiz のエラーコード。
-- 'imageData' が 'imageStride' と高さに足りなければ 'juizImageFormatError' を返す
writeImage :: Ptr CCapsule -> Image -> IO Int64
writeImage cap img
  | imageWidth img < 0 || imageHeight img < 0 || imageStride img < imageRowBytes img = pure juizImageFormatError
  | BS.length (imageData img) < needed = pure juizImageFormatError
  | otherwise =
      BSU.unsafeUseAsCString (imageData img) $ \dat ->
        allocaBytes imageStructSize $ \p -> do
          pokeByteOff p 0 (fromIntegral (imageWidth img) :: Word32)
          pokeByteOff p 4 (fromIntegral (imageHeight img) :: Word32)
          pokeByteOff p 8 (formatCode (imageFormat img))
          pokeByteOff p 12 (0 :: Word32)
          pokeByteOff p offStride (fromIntegral (imageStride img) :: CSize)
          pokeByteOff p offData dat
          c_capsule_set_image cap (castPtr p)
  where
    needed
      | imageHeight img == 0 = 0
      | otherwise = imageStride img * (imageHeight img - 1) + imageRowBytes img
