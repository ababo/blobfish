import logging

from tornado.websocket import WebSocketHandler

class SegmentHandler(WebSocketHandler):
    def open(self):
        logging.info('open')

    def on_message(self, message):
        logging.info(f'message {len(message)} bytes')
        # self.write_message(message)

    def on_close(self):
        logging.info("close")
