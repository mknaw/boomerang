zigbuild:
        ulimit -n 10000
        cargo zigbuild --release --target aarch64-unknown-linux-musl
