all: debug

# Build all targets in debug mode.
debug:
	cargo build --release
	@echo "Output files were compiled to the folder: target/debug"

# Build all targets in release mode
release:
	cargo build --release
	@echo "Output files were compiled to the folder: target/release"
