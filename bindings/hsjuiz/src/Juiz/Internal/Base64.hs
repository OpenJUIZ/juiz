-- | base64 (RFC 4648 §4、パディングあり)。データ型の bytes を辞書で表すのに使う (docs/spec/datatype.md §4)
module Juiz.Internal.Base64
  ( encodeBase64
  , decodeBase64
  ) where

import Data.Bits (shiftL, shiftR, (.&.), (.|.))
import qualified Data.ByteString as BS
import qualified Data.ByteString.Char8 as BC
import Data.Char (chr, ord)
import Data.Word (Word8)

alphabet :: BS.ByteString
alphabet = BC.pack "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"

encodeBase64 :: BS.ByteString -> String
encodeBase64 bs = go (BS.unpack bs)
  where
    sym :: Int -> Char
    sym i = chr (fromIntegral (BS.index alphabet i))
    go (a : b : c : rest) =
      let n = w a `shiftL` 16 .|. w b `shiftL` 8 .|. w c
      in map (\s -> sym ((n `shiftR` s) .&. 63)) [18, 12, 6, 0] ++ go rest
    go [a, b] =
      let n = w a `shiftL` 16 .|. w b `shiftL` 8
      in map (\s -> sym ((n `shiftR` s) .&. 63)) [18, 12, 6] ++ "="
    go [a] =
      let n = w a `shiftL` 16
      in map (\s -> sym ((n `shiftR` s) .&. 63)) [18, 12] ++ "=="
    go [] = []
    w :: Word8 -> Int
    w = fromIntegral

-- | パディングまで正しい base64 だけを受け付ける。誤りなら Nothing
decodeBase64 :: String -> Maybe BS.ByteString
decodeBase64 s
  | length s `mod` 4 /= 0 = Nothing
  | otherwise = BS.pack . concat <$> mapM quad (chunks s)
  where
    chunks [] = []
    chunks xs = let (a, b) = splitAt 4 xs in a : chunks b
    value c
      | c >= 'A' && c <= 'Z' = Just (ord c - ord 'A')
      | c >= 'a' && c <= 'z' = Just (ord c - ord 'a' + 26)
      | c >= '0' && c <= '9' = Just (ord c - ord '0' + 52)
      | c == '+' = Just 62
      | c == '/' = Just 63
      | otherwise = Nothing
    -- 使わない下位のビットが 0 でなければ誤り (同じバイト列の表し方を 1 つにする)
    bytes :: Int -> [Int] -> Maybe [Word8]
    bytes count vs
      | n .&. ((1 `shiftL` (8 * (3 - count))) - 1) /= 0 = Nothing
      | otherwise = Just [fromIntegral ((n `shiftR` (16 - 8 * i)) .&. 255) | i <- [0 .. count - 1]]
      where
        n = combine vs
    quad [a, b, '=', '='] = mapM value [a, b] >>= bytes 1
    quad [a, b, c, '='] = mapM value [a, b, c] >>= bytes 2
    quad q = mapM value q >>= bytes 3
    combine vs = foldl (\acc v -> acc `shiftL` 6 .|. v) 0 vs `shiftL` (6 * (4 - length vs))
