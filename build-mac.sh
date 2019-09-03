#!/bin/sh

cd "`dirname "$0"`"

cargo build --release -p stella2 || exit $?
cargo run --bin mkmacosbundle || exit $?
