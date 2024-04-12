from typing import Self
import logging

from tornado.websocket import WebSocketHandler


class SegmentHandler(WebSocketHandler):
    def open(self):
        logging.info('open')

        content_type = self.request.headers.get('Content-Type')
        logging.info(f'headers {self.request.headers}')
        if content_type != 'audio/lpcm':
            CLOSE_UNSUPPORTED = 1003
            self.close(CLOSE_UNSUPPORTED, 'unsupported audio type')

    def on_message(self, message):
        logging.info(f'message {len(message)} bytes')
        # self.write_message(message)

    def on_close(self):
        logging.info("close")
