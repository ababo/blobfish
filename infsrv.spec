# -*- mode: python ; coding: utf-8 -*-

from PyInstaller.utils.hooks import collect_data_files

scripts = [
    'infsrv/__main__.py'
]

datas = collect_data_files('lightning_fabric')

hiddenimports = [
    'pyannote.audio.models',
    'pyannote.audio.models.embedding',
    'pyannote.audio.models.segmentation',
    'pyannote.audio.pipelines',
    'tornado.web'
]

analysis = Analysis(  # type: ignore
    scripts,
    datas=datas,
    hiddenimports=hiddenimports,
    noarchive=False,
)

pyz = PYZ(analysis.pure)  # type: ignore

exe = EXE(  # type: ignore
    pyz,
    analysis.scripts,
    analysis.binaries,
    analysis.datas,
    [],
    name='infsrv',
    strip=False,
    upx=True,
)
