from http import HTTPStatus
from typing import Any

from tornado.httputil import HTTPServerRequest
from tornado.web import Application, HTTPError
from tornado.websocket import WebSocketHandler

import util

logger = util.add_logger('segment')

SAMPLE_SIZES = {'i16': 2, 'i32': 4, 'f32': 4}


class SegmentHandler(WebSocketHandler):
    def __init__(
        self,
        application: Application,
        request: HTTPServerRequest,
        **kwargs: Any
    ) -> None:
        WebSocketHandler.__init__(self, application, request, **kwargs)

        self._setup_request_params()

        capacity = self.rolling_window_duration * self.num_channels * \
            self.sample_rate * SAMPLE_SIZES[self.sample_type] // 1000
        self._buffer = CircularBuffer(capacity)

        self._bytes_written = 0

    def _setup_request_params(self):
        try:
            self.rolling_window_duration = int(
                self.get_query_argument('rwd', '5000'))
            if self.rolling_window_duration < 100 or \
                    self.rolling_window_duration > 10000:
                raise Exception
        except:
            raise HTTPError(HTTPStatus.BAD_REQUEST,
                            "malformed or unsupported 'rwd' "
                            '(rolling window duration msecs) query parameter')

        try:
            self.rolling_window_step = int(
                self.get_query_argument('rws', '1000'))
            if self.rolling_window_step < 100 or \
                    self.rolling_window_step > 10000:
                raise Exception
        except:
            raise HTTPError(HTTPStatus.BAD_REQUEST,
                            "malformed or unsupported 'rws' "
                            '(rolling window step msecs) query parameter')

        try:
            self.num_channels = int(self.get_query_argument('nc'))
            if self.num_channels < 1 or self.num_channels > 8:
                raise Exception
        except:
            raise HTTPError(HTTPStatus.BAD_REQUEST,
                            'missing or malformed or unsupported '
                            "'nc' (number of channels) query parameter")

        try:
            self.sample_rate = int(self.get_query_argument('sr'))
            if self.sample_rate < 8000 or self.sample_rate > 192000:
                raise Exception
        except:
            raise HTTPError(HTTPStatus.BAD_REQUEST,
                            'missing, malformed or unsupported '
                            "'sr' (sample rate) query parameter")

        self.sample_type = self.get_query_argument('st')
        if self.sample_type not in SAMPLE_SIZES.keys():
            raise HTTPError(HTTPStatus.BAD_REQUEST,
                            "missing or unknown 'st' (sample type) "
                            "query parameter, expected 'i16', 'i32' or 'f32'")

        self.content_type = self.request.headers.get('Content-Type')
        if self.content_type != 'audio/lpcm':
            raise HTTPError(HTTPStatus.BAD_REQUEST,
                            "unsupported audio type, expected 'audio/lpcm'")

    def open(self) -> None:
        logger.info('open')

    def on_message(self, message) -> None:
        logger.info(f'message {len(message)} bytes')
        if type(message) != bytes:
            return

        sample_size = SAMPLE_SIZES[self.sample_type]
        num_step_bytes = self.rolling_window_step * self.num_channels * \
            self.sample_rate * sample_size // 1000

        step_remainder = self._bytes_written % num_step_bytes
        self._bytes_written += len(message)

        while step_remainder + len(message) >= num_step_bytes:
            num_bytes_to_add = num_step_bytes-step_remainder
            step_remainder = 0
            self._buffer.add(message[:num_bytes_to_add])
            self._process_step()
            message = message[num_bytes_to_add:]

        self._buffer.add(message)

    def on_close(self) -> None:
        logger.info("close")

    def _process_step(self) -> None:
        logger.info("_process_step")


class CircularBuffer:
    def __init__(self, capacity) -> Any:
        self._data = bytearray(capacity)
        self._length = 0
        self._from = 0

    def __len__(self) -> int:
        return self._length

    def add(self, data: bytearray | bytes) -> Any:
        data = data[-min(len(data), len(self._data)):]
        to = (self._from + self._length) % len(self._data)
        until_wraparound = min(len(data), len(self._data)-to)
        self._data[to:to+until_wraparound] = data[:until_wraparound]
        self._data[:len(data)-until_wraparound] = data[until_wraparound:]
        self._from = max(len(data)-until_wraparound, self._from)
        self._length = min(self._length+len(data), len(self._data))

    def data(self) -> bytearray:
        shifted = self._data[self._from:]+self._data[:self._from]
        return shifted[:self._length]
