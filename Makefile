.PHONY: build release test clean install

TARGET_AARCH64 = aarch64-apple-darwin
TARGET_X86_64  = x86_64-apple-darwin

VERSION := $(shell cargo metadata --no-deps --format-version 1 \
	| python3 -c "import sys,json; print(json.load(sys.stdin)['packages'][0]['version'])")

build:
	cargo build

test:
	cargo test

install:
	cargo install --path .

# Build a universal binary and create a release tarball.
release:
	rustup target add $(TARGET_AARCH64)
	rustup target add $(TARGET_X86_64)
	cargo build --release --target $(TARGET_AARCH64)
	cargo build --release --target $(TARGET_X86_64)
	lipo -create \
		target/$(TARGET_AARCH64)/release/swt \
		target/$(TARGET_X86_64)/release/swt \
		-output target/swt-universal
	mkdir -p dist
	cp target/swt-universal dist/swt
	tar -czf dist/sparse-worktree-$(VERSION).tar.gz -C dist swt
	@echo "SHA256: $$(shasum -a 256 dist/sparse-worktree-$(VERSION).tar.gz | awk '{print $$1}')"
	@echo "Release tarball: dist/sparse-worktree-$(VERSION).tar.gz"

clean:
	cargo clean
	rm -rf dist
