export PYTHONDONTWRITEBYTECODE=1

.PHONY: pkg-infsrv
pkg-infsrv:
	pyinstaller infsrv.spec --distpath target --workpath target/infsrv-build

.PHONY: run-infsrv
run-infsrv:
	python infsrv

.PHONY: test-infsrv
test-infsrv:
	pytest infsrv/test
