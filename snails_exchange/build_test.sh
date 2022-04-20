#export RUST_BACKTRACE=full
cd ../test-token/
./build.sh
cd ../snails_exchange 
cargo test -- --nocapture
