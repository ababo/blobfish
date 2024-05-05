export PYTHONDONTWRITEBYTECODE=1

.PHONY: lint-infsrv
lint-infsrv:
	pylint infsrv

.PHONY: pkg-infsrv
pkg-infsrv:
	pyinstaller infsrv.spec --distpath target --workpath target/infsrv-build

.PHONY: run-bfsrv
run-bfsrv:
	INFSRV_URL=ws://127.0.0.1:8001/segment \
	RUST_LOG=debug \
	SERVER_ADDRESS=127.0.0.1:8000 \
	cargo run --release

.PHONY: run-infsrv
run-infsrv:
	LOG_LEVEL=debug \
	SERVER_PORT=8001 \
	python infsrv

.PHONY: test-infsrv
test-infsrv:
	pytest infsrv/test -vv -l -p no:cacheprovider
