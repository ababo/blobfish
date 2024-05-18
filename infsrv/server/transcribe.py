"""Speech transcription."""

import asyncio
from concurrent.futures import ThreadPoolExecutor
from typing import Dict, List

from fastapi import File, Form, Header, HTTPException, UploadFile, status
from fastapi.responses import JSONResponse

from capability import CapabilitySet
from faster_whisper import WhisperModel
from server.common import (
    CAPABILITIES_HEADER, find_request_capability
)


class TranscribeHandler:  # pylint: disable=too-few-public-methods
    """Handler for speech transcription requests."""

    def __init__(
            self,
            executor: ThreadPoolExecutor,
            capabilities: List[str]
    ) -> None:
        self._executor = executor

        self._models: Dict[str, WhisperModel] = {}
        module_capabilities = CapabilitySet.get(). \
            module_capabilities('server/transcribe')
        for name, capability in module_capabilities.items():
            if name in capabilities:
                model = WhisperModel(capability.model_load_path,
                                     compute_type=capability.compute_type,
                                     device=capability.compute_device)
                self._models[name] = model

    async def endpoint(
        self,
        capabilities: str = Header(..., alias=CAPABILITIES_HEADER),
        file: UploadFile = File(...),
        prompt: str = Form(default=None),
        language: str = Form(default=None),
        temperature: str | None = Form(default=None),
    ) -> None:
        """Speech transcription endpoint."""
        # pylint: disable=too-many-arguments
        capability = find_request_capability(
            self._models.keys(), capabilities)
        model = self._models[capability]
        beam_size = CapabilitySet.get().capabilities[capability].beam_size

        if language is not None and language not in model.supported_languages:
            raise HTTPException(status.HTTP_400_BAD_REQUEST,
                                'bad or unsupported language')

        if temperature is None:
            temperatures = [0.0, 0.2, 0.4, 0.6, 0.8, 1.0]
        else:
            try:
                temperatures = list(map(float, temperature.split(',')))
            except ValueError as err:
                raise HTTPException(
                    status.HTTP_400_BAD_REQUEST,
                    'malformed temperature form parameter') from err

        def task():
            return model.transcribe(
                file.file,
                beam_size=beam_size,
                initial_prompt=prompt,
                language=language,
                temperature=temperatures,
            )

        loop = asyncio.get_event_loop()
        segments, _ = await loop.run_in_executor(self._executor, task)
        text = ''.join(map(lambda s: s.text, segments))
        return JSONResponse(content={'text': text})
