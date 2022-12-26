debug: setup_dev
	cargo build
	cp target/debug/libmuzzman_module_transport.so libtransport.so

release: setup_dev
	cargo build --release
	cp target/release/libmuzzman_module_transport.so libtransport.so

clean:
	cargo clean

setup_dev:
