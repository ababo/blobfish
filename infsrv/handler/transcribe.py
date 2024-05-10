"""Speech transcription handler."""

from http import HTTPStatus
from io import BytesIO
import json
from typing import Dict, List

from capability import CapabilitySet
from faster_whisper import WhisperModel
from handler import (
    CONTENT_TYPE_HEADER, CONTENT_TYPE_JSON,
    find_request_capability, run_sync_task
)
from tornado.web import RequestHandler


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


class TranscribeHandler(RequestHandler):  # pylint: disable=abstract-method
    """Speech to text conversion handler."""

    async def post(self):
        """Handle transcription request."""
        capability = find_request_capability(_whisper_models.keys(), self)
        if capability is None:
            return

        content_type = self.request.headers.get(CONTENT_TYPE_HEADER, '')
        if 'multipart/form-data' not in content_type:
            self.set_status(HTTPStatus.BAD_REQUEST)
            self.finish(
                "unsupported content type, expected 'multipart/form-data'")
            return

        # Make it partially compatible with OpenAI transcriptions API, see
        # https://platform.openai.com/docs/api-reference/audio/createTranscription.

        files = self.request.files.get('file', [])
        if len(files) == 0:
            self.set_status(HTTPStatus.BAD_REQUEST)
            self.finish("no 'file' (audio file object) provided")
            return

        model = _whisper_models[capability]
        beam_size = CapabilitySet.get().capabilities[capability].beam_size
        prompt = self.get_body_argument('prompt', None)

        language = self.get_body_argument('language', None)
        if language is not None and language not in model.supported_languages:
            self.set_status(HTTPStatus.BAD_REQUEST)
            self.finish('bad or unsupported language')
            return

        try:
            temperature = float(self.get_body_argument('temperature', 0))
        except ValueError:
            self.set_status(HTTPStatus.BAD_REQUEST)
            self.finish('malformed temperature')
            return

        def task():
            return model.transcribe(
                BytesIO(files[0].body),
                beam_size=beam_size,
                initial_prompt=prompt,
                language=language,
                temperature=temperature)

        segments, _ = await run_sync_task(task)
        text = ' '.join(map(lambda s: s.text, segments))

        self.set_header(CONTENT_TYPE_HEADER, CONTENT_TYPE_JSON)
        self.write(json.dumps({'text': text}, ensure_ascii=False))
