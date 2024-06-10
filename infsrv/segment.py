"""Speech segmentation logic."""

from dataclasses import dataclass
from typing import Awaitable, Callable, List, Tuple

from dataclasses_json import dataclass_json


class ChunkDivider:  # pylint: disable=too-few-public-methods
    """Splits incoming byte chunks into fixed-size parts."""

    def __init__(self, length: int,
                 callback: Callable[[bytes, bool], Awaitable[None]]) -> None:
        self._buffer = bytearray(length)
        self._callback = callback
        self._index = 0

    async def add(self, chunk: bytes | bytearray, last: bool = False) -> None:
        """Process a new chunk.
        This method might call the given callback one or more times.
        """
        while self._index + len(chunk) >= len(self._buffer):
            extent = len(self._buffer) - self._index
            self._buffer[self._index:] = chunk[:extent]
            self._index = 0
            await self._callback(bytes(self._buffer), False)
            chunk = chunk[extent:]

        self._buffer[self._index:self._index+len(chunk)] = chunk
        self._index += len(chunk)

        if last and self._index > 0:
            await self._callback(bytes(self._buffer[:self._index]), True)


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

    def duration(self):
        """Segment duration."""
        return self.end - self.begin


# pylint: disable=too-few-public-methods
# pylint: disable=too-many-instance-attributes
class SegmentProducer:
    """Converts in-window intervals into continuous time segments."""

    def __init__(
        self,
        window_duration: float,
        min_speech_duration: float,
        max_segment_duration: float,
        time_epsilon: float,
    ) -> None:
        assert min_speech_duration <= max_segment_duration

        self._window_duration = window_duration
        self._min_speech_duration = min_speech_duration
        self._max_segment_duration = max_segment_duration
        self._time_epsilon = time_epsilon

        self._trailing_begin = 0
        self._trailing_kind = KIND_VOID
        self._time_offset = 0

        self._trailing_segment: Segment | None = None

    def next_window(
        self,
        intervals: List[Tuple[float, float]],
        last: bool = False
    ) -> List[Segment]:
        """Add next window intervals and return next ready-made segments."""
        window_end = self._time_offset + self._window_duration

        segments = []
        if len(intervals) == 0:
            # close a trailing segment and add a void window
            _append_segment(segments, self._trailing_kind,
                            self._trailing_begin, self._time_offset)
            _append_segment(segments, KIND_VOID,
                            self._time_offset, window_end)
            self._trailing_kind = KIND_VOID
            self._trailing_begin = window_end

        for begin, end in intervals:
            open_end = end > self._window_duration - self._time_epsilon
            if begin < self._time_epsilon:  # open begin
                if open_end:
                    break
                _append_segment(segments, KIND_SPEECH,
                                self._trailing_begin,
                                self._time_offset + end)
                self._trailing_begin = self._time_offset + end
                self._trailing_kind = KIND_VOID
                continue

            if open_end:
                _append_segment(segments, self._trailing_kind,
                                self._trailing_begin,
                                self._time_offset + begin)
                self._trailing_begin = self._time_offset + begin
                self._trailing_kind = KIND_SPEECH
                continue

            _append_segment(segments, self._trailing_kind,
                            self._trailing_begin,
                            self._time_offset + begin)
            _append_segment(segments, KIND_SPEECH,
                            self._time_offset + begin,
                            self._time_offset + end)
            self._trailing_begin = self._time_offset + end
            self._trailing_kind = KIND_VOID

        if self._trailing_kind == KIND_VOID:  # append trailing void segment
            begin = segments[-1].end \
                if len(segments) > 0 else self._time_offset
            _append_segment(segments, KIND_VOID, begin, window_end)
            self._trailing_begin = window_end
        else:  # avoid carrying too long trailing speech segments
            while window_end - self._trailing_begin \
                    > self._max_segment_duration:
                end = self._trailing_begin + self._max_segment_duration
                _append_segment(
                    segments, KIND_SPEECH, self._trailing_begin, end)
                self._trailing_begin = end

        if last and self._trailing_kind == KIND_SPEECH:
            _append_segment(segments, KIND_SPEECH,
                            self._trailing_begin, window_end)

        segments = self._merge_segments(segments, last)
        _split_segments(segments, self._max_segment_duration)

        self._time_offset += self._window_duration
        return segments

    def _merge_segments(
        self,
        segments: List[Segment],
        last: bool
    ) -> List[Segment]:
        merged_segments = []
        for segment in segments:
            if self._trailing_segment is not None:
                self._trailing_segment.end = segment.end
                if self._trailing_segment.duration() >= \
                        self._min_speech_duration:
                    merged_segments.append(self._trailing_segment)
                    self._trailing_segment = None
            elif segment.kind == KIND_SPEECH and \
                    segment.duration() < self._min_speech_duration:
                self._trailing_segment = segment
            else:
                merged_segments.append(segment)

        if last and self._trailing_segment is not None:
            merged_segments.append(self._trailing_segment)

        return merged_segments


def _append_segment(segments: List[Segment],
                    kind: str, begin: int, end: int) -> None:
    if begin == end:
        return

    if len(segments) > 0:
        last = segments[-1]
        if last.kind == kind and last.end == begin:
            last.end = end
            return

    segments.append(Segment(kind, begin, end))


def _split_segments(
    segments: List[Segment],
    max_segment_duration: float,
) -> None:
    index = 0
    while index < len(segments):
        segment = segments[index]
        if segment.end - segment.begin > max_segment_duration:
            end = segment.begin + max_segment_duration
            segments.insert(index + 1,
                            Segment(segment.kind, end, segment.end))
            segment.end = end
        index += 1
