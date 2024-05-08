"""Speech segmentation logic."""

from typing import Awaitable, Callable, Iterable, List, Tuple


class ChunkDivider: # pylint: disable=too-few-public-methods
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


class SegmentProducer: # pylint: disable=too-few-public-methods
    """Converts in-window intervals into continuous time segments."""

    def __init__(self, window_duration: int, time_epsilon: int) -> None:
        self._window_duration = window_duration
        self._time_epsilon = time_epsilon
        self._trailing_begin = None
        self._time_offset = 0

    def next_window(
        self,
        intervals: Iterable[Tuple[int, int]]
    ) -> List[Tuple[int, int]]:
        """Add next window intervals and return next ready-made segments."""
        segments = []
        for begin, end in intervals:
            trailing = end > self._window_duration - self._time_epsilon
            if self._trailing_begin is not None:
                if begin < self._time_epsilon:
                    if trailing:
                        break
                    segments.append(
                        (self._trailing_begin, self._time_offset + end))
                    self._trailing_begin = None
                    continue
                segments.append((self._trailing_begin, self._time_offset))
                self._trailing_begin = None

            if trailing:
                self._trailing_begin = self._time_offset + begin
                break

            segments.append(
                (self._time_offset + begin, self._time_offset + end))

        self._time_offset += self._window_duration
        return segments
