.PHONY: pkg-infsrv
pkg-infsrv:
	pyinstaller infsrv/pyi.spec --distpath target --workpath target/pyi-build
