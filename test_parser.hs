{-# LANGUAGE OverloadedStrings #-}
import Text.Megaparsec
import Text.Megaparsec.Char
import qualified Text.Megaparsec.Char.Lexer as L
import Control.Monad.Combinators.Expr
import Data.Void
import Data.Text (Text)
import qualified Data.Text as T

type Parser = Parsec Void Text

data Expr = Num Int | Op Expr String Expr deriving (Show)

num :: Parser Expr
num = Num <$> L.decimal

operators :: [[Operator Parser Expr]]
operators = 
  [ [InfixL (Op <$ string "+") ]
  ]

expr :: Parser Expr
expr = makeExprParser num operators

main :: IO ()
main = do
  putStrLn "Testing: 1+2+3"
  print $ parse expr "" "1+2+3"
  putStrLn "\nTesting: 1+2+3+4"
  print $ parse expr "" "1+2+3+4"
