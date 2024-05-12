export PYTHONDONTWRITEBYTECODE=1

.PHONY: lint-infsrv
lint-infsrv:
	pylint infsrv

.PHONY: run-bfsrv
run-bfsrv:
	RUST_LOG=debug \
	cargo run --release

.PHONY: run-infsrv
run-infsrv:
	CAPABILITIES=segment-cpu,transcribe-small-cpu \
	LOG_LEVEL=debug \
	python infsrv

.PHONY: test-infsrv
test-infsrv:
	pytest infsrv/test -vv -l -p no:cacheprovider
