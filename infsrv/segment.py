from http import HTTPStatus
import logging
from typing import Any

from tornado.httputil import HTTPServerRequest
from tornado.web import Application, HTTPError
from tornado.websocket import WebSocketHandler

import util

logger = util.add_logger('segment')


class SegmentHandler(WebSocketHandler):
    def __init__(
        self,
        application: Application,
        request: HTTPServerRequest,
        **kwargs: Any
    ) -> None:
        WebSocketHandler.__init__(self, application, request, **kwargs)

        try:
            num_channels = int(self.get_query_argument('nc'))
            if num_channels < 1 or num_channels > 8:
                raise Exception
        except:
            raise HTTPError(HTTPStatus.BAD_REQUEST,
                            'missing or malformed or unsupported '
                            "'nc' (number of channels) query parameter")

        try:
            sample_rate = int(self.get_query_argument('sr'))
            if sample_rate < 8000 or sample_rate > 192000:
                raise Exception
        except:
            raise HTTPError(HTTPStatus.BAD_REQUEST,
                            'missing, malformed or unsupported '
                            "'sr' (sample rate) query parameter")

        sample_type = self.get_query_argument('st')
        if sample_type not in ['i16', 'i32', 'f32']:
            raise HTTPError(HTTPStatus.BAD_REQUEST,
                            "missing or unknown 'st' (sample type) "
                            "query parameter, expected 'i16', 'i32' or 'f32'")

        content_type = self.request.headers.get('Content-Type')
        if content_type != 'audio/lpcm':
            raise HTTPError(HTTPStatus.BAD_REQUEST,
                            "unsupported audio type, expected 'audio/lpcm'")

    def open(self) -> None:
        logging.info('open')

    def on_message(self, message) -> None:
        logger.info(f'message {len(message)} bytes')
        # self.write_message(message)

    def on_close(self) -> None:
        logger.info("close")
