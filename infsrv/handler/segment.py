from http import HTTPStatus
import json
from typing import Any

from pyannote.audio import Pipeline
from pyannote.core.annotation import Annotation
import torch
from tornado.httputil import HTTPServerRequest
from tornado.web import Application, HTTPError
from tornado.websocket import WebSocketHandler

import util

SAMPLE_SIZES = {'i16': 2, 'i32': 4, 'f32': 4}
SAMPLE_DTYPES = {'i16': torch.int16, 'i32': torch.int32, 'f32': torch.float32}

TIME_EPSILON = 100

logger = util.add_logger('segment')

pyannote_pipeline: None | Pipeline = None


def load_pyannote(config_file: str, torch_device: str) -> None:
    global pyannote_pipeline
    pyannote_pipeline = Pipeline.from_pretrained(config_file)
    pyannote_pipeline.to(torch.device(torch_device))


class SegmentHandler(WebSocketHandler):
    def __init__(
        self,
        application: Application,
        request: HTTPServerRequest,
        **kwargs: Any
    ) -> None:
        WebSocketHandler.__init__(self, application, request, **kwargs)

        self._setup_request_params()

        window_buffer_len = self._window_duration * self._num_channels * \
            self._sample_rate * SAMPLE_SIZES[self._sample_type] // 1000
        self._window_buffer = bytearray(window_buffer_len)
        self._window_buffer_index = 0

        self._time_offset = 0
        self._trailing_begin = None

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
        if self._sample_type not in SAMPLE_SIZES.keys():
            raise HTTPError(HTTPStatus.BAD_REQUEST,
                            "missing or unknown 'st' (sample type) "
                            "query parameter, expected 'i16', 'i32' or 'f32'")

        content_type = self.request.headers.get('Content-Type')
        if content_type != 'audio/lpcm':
            raise HTTPError(HTTPStatus.BAD_REQUEST,
                            "unsupported audio type, expected 'audio/lpcm'")

    def open(self) -> None:
        logger.debug('open /segment')
        pass

    def on_message(self, message) -> None:
        if type(message) != bytes:
            return

        window_len = len(self._window_buffer)
        while self._window_buffer_index + len(message) >= window_len:
            extent = window_len - self._window_buffer_index
            self._window_buffer[self._window_buffer_index:] = message[:extent]
            self._window_buffer_index = 0
            self._process_window()
            message = message[extent:]

        index = self._window_buffer_index
        self._window_buffer[index:index+len(message)] = message
        self._window_buffer_index += len(message)

    def on_close(self) -> None:
        logger.debug('on_close /segment')

    def _process_window(self) -> None:
        dtype = SAMPLE_DTYPES[self._sample_type]
        waveform = torch.frombuffer(self._window_buffer, dtype=dtype)
        waveform = waveform.reshape((-1, self._num_channels))
        waveform = torch.transpose(waveform, 0, 1)
        waveform = torch.mean(waveform.float(), dim=0, keepdim=True)
        if not dtype.is_floating_point:
            sample_size = SAMPLE_SIZES[self._sample_type]
            waveform /= 2 ** (sample_size * 8 - 1) - 1
        audio = {'waveform': waveform, 'sample_rate': self._sample_rate}
        annotation: Annotation = pyannote_pipeline(audio)

        for segment, _ in annotation.itertracks():
            begin = int(segment.start * 1000)
            end = int(segment.end * 1000)

            trailing = abs(self._window_duration - end) < TIME_EPSILON
            if self._trailing_begin is not None:
                if begin < TIME_EPSILON:
                    if trailing:
                        break
                    self._write_segment(self._trailing_begin,
                                        self._time_offset + end)
                    self._trailing_begin = None
                    continue
                else:
                    self._write_segment(
                        self._trailing_begin, self._time_offset)
                    self._trailing_begin = None

            if trailing:
                self._trailing_begin = self._time_offset + begin
                break

            self._write_segment(self._time_offset + begin,
                                self._time_offset + end)

        self._time_offset += self._window_duration

    def _write_segment(self, begin: int, end: int):
        logger.debug(f'written segment {begin}ms-{end}ms')
        self.write_message(json.dumps({'begin': begin, 'end': end}) + '\n')
