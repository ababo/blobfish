export PYTHONDONTWRITEBYTECODE=1

.PHONY: lint-infsrv
lint-infsrv:
	pylint infsrv

.PHONY: run-bfsrv
run-bfsrv:
	RUST_LOG=debug \
	PAYPAL_RETURN_URL=https://run.mocky.io/v3/f2b62cfc-f607-43ec-b876-ffced783a229 \
	PAYPAL_CANCEL_URL=https://run.mocky.io/v3/9c4d1368-40af-4b6c-bf71-a4170c98eb85 \
	cargo run --release

.PHONY: run-infsrv
run-infsrv:
	CAPABILITIES=segment-cpu,transcribe-small-cpu \
	LOG_LEVEL=debug \
	python infsrv

.PHONY: test-infsrv
test-infsrv:
	pytest infsrv/test -vv -l -p no:cacheprovider
