export PYTHONDONTWRITEBYTECODE=1

.PHONY: build-api-spec
build-api-spec:
	npx @redocly/cli build-docs bfsrv/api.oas.json -o target/api.oas.html
	open target/api.oas.html

.PHONY: deploy-api-spec
deploy-api-spec:
	npx @redocly/cli build-docs bfsrv/api.oas.json -o target/api.oas.html
	scp target/api.oas.html root@blobfish.no:/home/user-data/www/default/api-spec.html

.PHONY: lint-infsrv
lint-infsrv:
	pylint infsrv

.PHONY: run-bfsrv
run-bfsrv:
	RUST_LOG=debug,tokio_postgres=info \
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
