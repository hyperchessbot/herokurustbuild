#!/bin/bash

# download engine
wget https://s3-us-west-2.amazonaws.com/variant-stockfish/ddugovic/master/stockfish-x86_64 -O stockfish12
chmod +x stockfish12

# download book
wget https://raw.githubusercontent.com/hyperchessbot/pgnrepo/main/rustbot.pgn -O book.pgn

# start bot
./target/release/hello
