"""Speech transcription handler."""

from http import HTTPStatus
from typing import Dict, List

from fastapi import File, Form, Header, HTTPException, UploadFile
from fastapi.responses import JSONResponse

from capability import CapabilitySet
from faster_whisper import WhisperModel
from handler import (
    CAPABILITIES_HEADER, find_request_capability, run_sync_task
)

_whisper_models: Dict[str, WhisperModel] = {}


def init(capabilities: List[str]) -> None:
    """Create Whisper models."""
    module_capabilities = CapabilitySet.get(). \
        module_capabilities('handler/transcribe')
    for name, capability in module_capabilities.items():
        if name in capabilities:
            model = WhisperModel(capability.model_load_path,
                                 compute_type=capability.compute_type,
                                 device=capability.compute_device)
            _whisper_models[name] = model


async def handle_transcribe(
        capabilities: str = Header(..., alias=CAPABILITIES_HEADER),
        file: UploadFile = File(...),
        prompt: str = Form(default=None),
        language: str = Form(default=None),
        temperature: float = Form(default=0),
) -> None:
    """Speech to text conversion handler."""

    capability = find_request_capability(
        _whisper_models.keys(), capabilities)
    model = _whisper_models[capability]
    beam_size = CapabilitySet.get().capabilities[capability].beam_size

    if language is not None and language not in model.supported_languages:
        raise HTTPException(HTTPStatus.BAD_REQUEST,
                            'bad or unsupported language')

    def task():
        return model.transcribe(
            file.file,
            beam_size=beam_size,
            initial_prompt=prompt,
            language=language,
            temperature=temperature)

    segments, _ = await run_sync_task(task)
    text = ''.join(map(lambda s: s.text, segments))

    return JSONResponse(content={'text': text})
