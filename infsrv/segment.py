from typing import Self
import logging

from bitstring import ConstBitStream
from tornado.websocket import WebSocketHandler


class OggPage:
    raw_size = 0

    def from_raw_data(data: bytes) -> Self | None:
        stream = ConstBitStream(data)

        HEADER_SIZE = 27
        if len(stream.bytes) - stream.pos < HEADER_SIZE:
            return None

        magic = stream.read(32).hex
        if magic != '4f676753':
            raise ValueError('bad ogg magic')

        version = stream.read(8).uint
        if version != 0:
            raise ValueError('unknown ogg version')

        stream.pos += 5  # Reserved.

        page = OggPage()

        page.is_end_of_stream = stream.read(1).bool
        page.is_beginning_of_stream = stream.read(1).bool
        page.is_continuation = stream.read(1).bool

        page.granule_pos = stream.read(64).uintle
        page.bitstream_serial = stream.read(32).uintle
        page.page_seq_num = stream.read(32).uintle
        page.crc32 = stream.read(32).uintle

        num_segments = stream.read(8).uint

        if len(stream.bytes) - stream.pos < num_segments:
            return None

        segment_lens = []
        for i in range(num_segments):
            segment_len = stream.read(8).uint
            segment_lens.append(segment_len)

        segment_total_len = sum(segment_lens)
        if len(stream.bytes) - stream.pos < segment_total_len:
            return None

        page.segments = []
        for i in range(num_segments):
            segment = stream.read(8 * segment_lens[i]).bytes
            page.segments.append(segment)

        page.raw_size = HEADER_SIZE + num_segments + segment_total_len

        return page


class OggStream:
    _unparsed = bytearray()

    def add_raw_data(self, data: bytes) -> list[OggPage]:
        self._unparsed.extend(data)

        pages = []
        while True:
            page = OggPage.from_raw_data(self._unparsed)
            if page == None:
                return pages

            pages.append(page)
            self._unparsed = self._unparsed[page.raw_size:]


class SegmentHandler(WebSocketHandler):
    def open(self):
        logging.info('open')

        content_type = self.request.headers.get('Content-Type')
        logging.info(f'headers {self.request.headers}')
        if content_type != 'audio/opus':
            # See https://datatracker.ietf.org/doc/html/rfc6455#section-7.4.1
            CLOSE_UNSUPPORTED = 1003
            self.close(CLOSE_UNSUPPORTED, 'unsupported audio type')

        self.stream = OggStream()

    def on_message(self, message):
        logging.info(f'message {len(message)} bytes')

        pages = self.stream.add_raw_data(message)
        for page in pages:
            logging.info(f'ogg page {page}')

        # self.write_message(message)

    def on_close(self):
        logging.info("close")
