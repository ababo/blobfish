.PHONY: pkg-infsrv
pkg-infsrv:
	pyinstaller infsrv.spec --distpath target --workpath target/infsrv-build

.PHONY: run-infsrv
run-infsrv:
	python -B infsrv
