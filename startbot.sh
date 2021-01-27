#!/bin/bash

# download engine
wget https://s3-us-west-2.amazonaws.com/variant-stockfish/ddugovic/master/stockfish-x86_64 -O stockfish12
chmod +x stockfish12

# download book
wget https://raw.githubusercontent.com/hyperchessbot/pgnrepo/main/rustbot.pgn -O book.pgn

# list files
ls -al

# get restart interval in seconds ( default = 1800 )
RESTART="${RESTART:-1800}"

# run bot
while true; do
	echo "starting bot with restart interval $RESTART"
	./target/release/hello &
	sleep $RESTART
	echo "shutting down bot"
	kill $!
done
