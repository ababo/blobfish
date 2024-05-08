"""Speech segmentation handler."""

from http import HTTPStatus
import json
from typing import Dict, List, Tuple

from pyannote.audio import Pipeline
from pyannote.core.annotation import Annotation, Segment
from pyannote.core.utils.types import TrackName
import torch
from tornado.websocket import WebSocketHandler

from capability import CapabilitySet
from handler import (
    CONTENT_TYPE_HEADER,
    find_request_capability, run_sync_task
)
from segment import ChunkDivider, SegmentProducer
import util

_SAMPLE_SIZES = {'i16': 2, 'i32': 4, 'f32': 4}
_SAMPLE_DTYPES = {'i16': torch.int16, 'i32': torch.int32, 'f32': torch.float32}

_logger = util.add_logger('segment')

_pyannote_pipelines: Dict[str, Pipeline] = {}


def init(capabilities: List[str]) -> None:
    """Create pyannote pipelines."""
    module_capabilities = CapabilitySet.get(). \
        module_capabilities('handler/segment')
    for name, capability in module_capabilities.items():
        if name in capabilities:
            pipeline = Pipeline.from_pretrained(capability.model_load_path)
            pipeline.to(torch.device(capability.compute_device))
            _pyannote_pipelines[name] = pipeline


class SegmentHandler(WebSocketHandler):  # pylint: disable=abstract-method
    """Websocket handler for realtime audio segmentation."""

    _num_channels: int
    _sample_rate: int
    _sample_type: str
    _window_duration: int
    _pyannote_pipeline: Pipeline
    _chunk_divider: ChunkDivider
    _segment_producer: SegmentProducer

    def prepare(self) -> None:
        try:
            self._num_channels = int(self.get_query_argument('nc'))
            if self._num_channels < 1 or self._num_channels > 8:
                raise ValueError
        except ValueError:
            self.set_status(HTTPStatus.BAD_REQUEST)
            self.finish('missing, malformed or unsupported '
                        "'nc' (number of channels) query parameter")
            return

        try:
            self._sample_rate = int(self.get_query_argument('sr'))
            if self._sample_rate < 8000 or self._sample_rate > 192000:
                raise ValueError
        except ValueError:
            self.set_status(HTTPStatus.BAD_REQUEST)
            self.finish('missing, malformed or unsupported '
                        "'sr' (sample rate) query parameter")
            return

        self._sample_type = self.get_query_argument('st')
        if self._sample_type not in _SAMPLE_SIZES:
            self.set_status(HTTPStatus.BAD_REQUEST)
            self.finish("missing or unknown 'st' (sample type) "
                        "query parameter, expected 'i16', 'i32' or 'f32'")
            return

        try:
            self._window_duration = int(
                self.get_query_argument('wd', '5000'))
            if self._window_duration < 1000 or \
                    self._window_duration > 10000:
                raise ValueError
        except ValueError:
            self.set_status(HTTPStatus.BAD_REQUEST)
            self.finish("malformed or unsupported 'wd' "
                        '(window duration msecs) query parameter')
            return

        capability = find_request_capability(_pyannote_pipelines.keys(), self)
        if capability is None:
            return
        self._pyannote_pipeline = _pyannote_pipelines[capability]

        content_type = self.request.headers.get(CONTENT_TYPE_HEADER)
        if content_type != 'audio/lpcm':
            self.set_status(HTTPStatus.BAD_REQUEST)
            self.finish("unsupported audio type, expected 'audio/lpcm'")
            return

        window_buffer_len = self._window_duration * self._num_channels * \
            self._sample_rate * _SAMPLE_SIZES[self._sample_type] // 1000
        self._chunk_divider = ChunkDivider(
            window_buffer_len, self._chunk_divider_callback)

        self._segment_producer = SegmentProducer(self._window_duration, 100)

    async def _chunk_divider_callback(self, data) -> None:
        segments = await run_sync_task(self._process_window, data)
        for begin, end in segments:
            _logger.debug('written segment %dms-%dms', begin, end)
            await self.write_message(json.dumps({'begin': begin, 'end': end}) + '\n')

    def open(self, *_args: str, **_kwargs: str) -> None:
        _logger.debug('open /segment')

    async def on_message(self, message) -> None:
        # pylint: disable=invalid-overridden-method
        if isinstance(message, bytes):
            await self._chunk_divider.add(message)

    def on_close(self) -> None:
        _logger.debug('on_close /segment')

    def _process_window(self, data: bytes) -> List[Tuple[int, int]]:
        dtype = _SAMPLE_DTYPES[self._sample_type]
        device = self._pyannote_pipeline.device
        waveform = torch.frombuffer(data, dtype=dtype).to(device)
        waveform = waveform.reshape((-1, self._num_channels))
        waveform = torch.transpose(waveform, 0, 1)
        waveform = torch.mean(waveform.float(), dim=0, keepdim=True)
        if not dtype.is_floating_point:
            sample_size = _SAMPLE_SIZES[self._sample_type]
            waveform /= 2 ** (sample_size * 8 - 1) - 1
        audio = {'waveform': waveform, 'sample_rate': self._sample_rate}
        annotation: Annotation = self._pyannote_pipeline(audio)

        return self._segment_producer.next_window(
            map(_track_to_interval, annotation.itertracks()))


def _track_to_interval(track: Tuple[Segment, TrackName]) -> Tuple[int, int]:
    return (int(track[0].start * 1000), int(track[0].end * 1000))
