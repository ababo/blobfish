"""Speech segmentation logic."""

from dataclasses import dataclass
from typing import Awaitable, Callable, Iterable, List, Tuple

from dataclasses_json import dataclass_json


class ChunkDivider:  # pylint: disable=too-few-public-methods
    """Splits incoming byte chunks into fixed-size parts."""

    def __init__(self, length: int, callback: Callable[[bytes], Awaitable[None]]) -> None:
        self._buffer = bytearray(length)
        self._callback = callback
        self._index = 0

    async def add(self, chunk: bytes | bytearray) -> None:
        """Process a new chunk.
        This method might call the given callback one or more times.
        """
        while self._index + len(chunk) >= len(self._buffer):
            extent = len(self._buffer) - self._index
            self._buffer[self._index:] = chunk[:extent]
            self._index = 0
            await self._callback(bytes(self._buffer))
            chunk = chunk[extent:]

        self._buffer[self._index:self._index+len(chunk)] = chunk
        self._index += len(chunk)


KIND_SPEECH = 'speech'
KIND_VOID = 'void'


@dataclass_json
@dataclass
class Segment:
    """Time segment."""
    kind: str
    begin: float
    end: float

    def __init__(self, kind: str, begin: float, end: float) -> None:
        self.kind = kind
        self.begin = begin
        self.end = end


class SegmentProducer:  # pylint: disable=too-few-public-methods
    """Converts in-window intervals into continuous time segments."""

    def __init__(
        self,
        window_duration: float,
        time_epsilon: float,
        max_speech_duration: float
    ) -> None:
        self._window_duration = window_duration
        self._time_epsilon = time_epsilon
        self._max_speech_duration = max_speech_duration
        self._trailing_begin = None
        self._time_offset = 0

    def next_window(
        self,
        intervals: Iterable[Tuple[float, float]]
    ) -> List[Segment]:
        """Add next window intervals and return next ready-made segments."""
        intervals = list(intervals)
        window_end = self._time_offset + self._window_duration
        if len(intervals) == 0 and self._trailing_begin is not None:
            begin = self._trailing_begin
            self._trailing_begin = None
            segments = [
                Segment(KIND_SPEECH, begin, self._time_offset),
                Segment(KIND_VOID, self._time_offset, window_end)
            ]
            self._time_offset += self._window_duration
            return segments

        segments = []
        for begin, end in intervals:
            trailing = end > self._window_duration - self._time_epsilon
            if self._trailing_begin is not None:
                if begin < self._time_epsilon:
                    if trailing:
                        break
                    segments.append(Segment(KIND_SPEECH,
                                            self._trailing_begin,
                                            self._time_offset + end))
                    self._trailing_begin = None
                    continue
                segments.append(Segment(KIND_SPEECH,
                                        self._trailing_begin,
                                        self._time_offset))
                self._trailing_begin = None

            if trailing:
                self._trailing_begin = self._time_offset + begin
                break

            segments.append(Segment(KIND_SPEECH,
                                    self._time_offset + begin,
                                    self._time_offset + end))

        index = 0
        while index < len(segments):
            segment = segments[index]
            if segment.end - segment.begin > self._max_speech_duration:
                end = segment.begin + self._max_speech_duration
                segments.insert(index + 1,
                                Segment(KIND_SPEECH, end, segment.end))
                segment.end = end
            index += 1

        while self._trailing_begin is not None and window_end - \
                self._trailing_begin > self._max_speech_duration:
            end = self._trailing_begin + self._max_speech_duration
            segments.append(Segment(KIND_SPEECH, self._trailing_begin, end))
            self._trailing_begin = end

        if self._trailing_begin is None:
            begin = segments[-1].end \
                if len(segments) != 0 else self._time_offset
            if begin < window_end:
                segments.append(Segment(KIND_VOID, begin, window_end))

        self._time_offset += self._window_duration
        return segments
