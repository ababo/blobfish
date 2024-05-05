from http import HTTPStatus
import json
from typing import Any, Tuple

from pyannote.audio import Pipeline
from pyannote.core.annotation import Annotation, Segment
from pyannote.core.utils.types import TrackName
import torch
from tornado.httputil import HTTPServerRequest
from tornado.web import Application, HTTPError
from tornado.websocket import WebSocketHandler

from segment import ChunkDivider, SegmentProducer
import util

_SAMPLE_SIZES = {'i16': 2, 'i32': 4, 'f32': 4}
_SAMPLE_DTYPES = {'i16': torch.int16, 'i32': torch.int32, 'f32': torch.float32}

_logger = util.add_logger('segment')

_pyannote_pipeline: None | Pipeline = None


def load_pyannote(config_file: str, torch_device: str) -> None:
    """Load a given pyannote.audio model."""
    global _pyannote_pipeline
    _pyannote_pipeline = Pipeline.from_pretrained(config_file)
    _pyannote_pipeline.to(torch.device(torch_device))


class SegmentHandler(WebSocketHandler):
    """Websocket handler for realtime audio segmentation."""

    def __init__(
        self,
        application: Application,
        request: HTTPServerRequest,
        **kwargs: Any
    ) -> None:
        WebSocketHandler.__init__(self, application, request, **kwargs)

        self._setup_request_params()

        window_buffer_len = self._window_duration * self._num_channels * \
            self._sample_rate * _SAMPLE_SIZES[self._sample_type] // 1000
        self._chunk_divider = ChunkDivider(
            window_buffer_len, self._process_window)

        self._segment_producer = SegmentProducer(self._window_duration, 100)

    def _setup_request_params(self):
        try:
            self._window_duration = int(
                self.get_query_argument('wd', '5000'))
            if self._window_duration < 1000 or \
                    self._window_duration > 10000:
                raise Exception
        except:
            raise HTTPError(HTTPStatus.BAD_REQUEST,
                            "malformed or unsupported 'wd' "
                            '(window duration msecs) query parameter')

        try:
            self._num_channels = int(self.get_query_argument('nc'))
            if self._num_channels < 1 or self._num_channels > 8:
                raise Exception
        except:
            raise HTTPError(HTTPStatus.BAD_REQUEST,
                            'missing, malformed or unsupported '
                            "'nc' (number of channels) query parameter")

        try:
            self._sample_rate = int(self.get_query_argument('sr'))
            if self._sample_rate < 8000 or self._sample_rate > 192000:
                raise Exception
        except:
            raise HTTPError(HTTPStatus.BAD_REQUEST,
                            'missing, malformed or unsupported '
                            "'sr' (sample rate) query parameter")

        self._sample_type = self.get_query_argument('st')
        if self._sample_type not in _SAMPLE_SIZES.keys():
            raise HTTPError(HTTPStatus.BAD_REQUEST,
                            "missing or unknown 'st' (sample type) "
                            "query parameter, expected 'i16', 'i32' or 'f32'")

        content_type = self.request.headers.get('Content-Type')
        if content_type != 'audio/lpcm':
            raise HTTPError(HTTPStatus.BAD_REQUEST,
                            "unsupported audio type, expected 'audio/lpcm'")

    def open(self) -> None:
        _logger.debug('open /segment')
        pass

    def on_message(self, message) -> None:
        if type(message) == bytes:
            self._chunk_divider.add(message)

    def on_close(self) -> None:
        _logger.debug('on_close /segment')

    def _process_window(self, data: bytes) -> None:
        dtype = _SAMPLE_DTYPES[self._sample_type]
        waveform = torch.frombuffer(data, dtype=dtype)
        waveform = waveform.reshape((-1, self._num_channels))
        waveform = torch.transpose(waveform, 0, 1)
        waveform = torch.mean(waveform.float(), dim=0, keepdim=True)
        if not dtype.is_floating_point:
            sample_size = _SAMPLE_SIZES[self._sample_type]
            waveform /= 2 ** (sample_size * 8 - 1) - 1
        audio = {'waveform': waveform, 'sample_rate': self._sample_rate}
        annotation: Annotation = _pyannote_pipeline(audio)

        segments = self._segment_producer.next_window(
            map(_track_to_interval, annotation.itertracks()))

        for begin, end in segments:
            _logger.debug(f'written segment {begin}ms-{end}ms')
            self.write_message(json.dumps({'begin': begin, 'end': end}) + '\n')


def _track_to_interval(track: Tuple[Segment, TrackName]) -> Tuple[int, int]:
    return (int(track[0].start * 1000), int(track[0].end * 1000))
