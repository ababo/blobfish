from tornado.websocket import WebSocketHandler


class SegmentHandler(WebSocketHandler):
    def open(self):
        print("open")

    def on_message(self, message):
        self.write_message(message)

    def on_close(self):
        print("close")
