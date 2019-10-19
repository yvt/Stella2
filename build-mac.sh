#!/bin/sh

cd "`dirname "$0"`"

xargo build --target x86_64-apple-darwin --release -p stella2 || exit $?
cargo run --bin mkmacosbundle || exit $?
