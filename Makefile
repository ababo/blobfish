export PYTHONDONTWRITEBYTECODE=1
export RUST_LOG=debug
export SERVER_ADDRESS=127.0.0.1:3030

.PHONY: pkg-infsrv
pkg-infsrv:
	pyinstaller infsrv.spec --distpath target --workpath target/infsrv-build

.PHONY: run-bfsrv
run-bfsrv:
	cargo run

.PHONY: run-infsrv
run-infsrv:
	python infsrv

.PHONY: test-infsrv
test-infsrv:
	pytest infsrv/test -vv -l -p no:cacheprovider
