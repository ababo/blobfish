"""Tests for segmentation logic."""

from typing import Callable, List, Tuple

import pytest

from segment import (
    ChunkDivider, Segment, SegmentProducer, KIND_SPEECH, KIND_VOID
)


def _create_chunk_divider_callback(
        parts: List[bytes]
) -> Tuple[Callable[[bytes, bool], None], Callable[[], None]]:
    index = 0

    async def callback(part: bytes, last: bool) -> None:
        nonlocal index
        assert parts[index] == part
        assert last == (index == len(parts)-1)
        index += 1

    def assert_consumed() -> bool:
        nonlocal index
        assert index == len(parts)

    return callback, assert_consumed


@pytest.mark.asyncio
async def test_chunk_divider() -> None:
    """Perform ChunkDivider sanity test."""
    parts = [b'abcd', b'efgh', b'ijkl', b'mn']
    callback, assert_consumed = _create_chunk_divider_callback(parts)

    divider = ChunkDivider(4, callback)
    await divider.add(b'abc')
    await divider.add(b'def')
    await divider.add(b'ghijklmn', last=True)

    assert_consumed()


def test_segment_producer() -> None:
    """Perform SegmentProducer sanity test."""
    producer = SegmentProducer(100, 150, 2)

    segments = producer.next_window([(0, 10), (20, 50), (75, 99)])  # 0-100
    assert segments == [Segment(KIND_SPEECH, 0, 10),
                        Segment(KIND_VOID, 10, 20),
                        Segment(KIND_SPEECH, 20, 50),
                        Segment(KIND_VOID, 50, 75)]

    segments = producer.next_window([(1, 15), (35, 70), (85, 110)])  # 100-200
    assert segments == [Segment(KIND_SPEECH, 75, 115),
                        Segment(KIND_VOID, 115, 135),
                        Segment(KIND_SPEECH, 135, 170),
                        Segment(KIND_VOID, 170, 185)]

    segments = producer.next_window([(0, 100)])  # 200-300
    assert not segments

    segments = producer.next_window([(25, 55), (65, 101)])  # 300-400
    assert segments == [Segment(KIND_SPEECH, 185, 335),
                        Segment(KIND_SPEECH, 335, 355),
                        Segment(KIND_VOID, 355, 365)]

    segments = producer.next_window([(1, 101)])  # 400-500
    assert not segments

    segments = producer.next_window([(1, 65), (70, 99)])  # 500-600
    assert segments == [Segment(KIND_SPEECH, 365, 515),
                        Segment(KIND_SPEECH, 515, 565),
                        Segment(KIND_VOID, 565, 570)]

    segments = producer.next_window([(1, 101)])  # 600-700
    assert not segments

    segments = producer.next_window([(1, 101)])  # 700-800
    assert segments == [Segment(KIND_SPEECH, 570, 720)]

    segments = producer.next_window([(1, 101)])  # 800-900
    assert segments == [Segment(KIND_SPEECH, 720, 870)]

    segments = producer.next_window([])  # 900-1000
    assert segments == [Segment(KIND_SPEECH, 870, 900),
                        Segment(KIND_VOID, 900, 1000)]

    segments = producer.next_window([])  # 1000-1100
    assert segments == [Segment(KIND_VOID, 1000, 1100)]

    segments = producer.next_window(
        [(20, 30), (50, 99)], last=True)  # 1100-1200
    assert segments == [Segment(KIND_VOID, 1100, 1120),
                        Segment(KIND_SPEECH, 1120, 1130),
                        Segment(KIND_VOID, 1130, 1150),
                        Segment(KIND_SPEECH, 1150, 1200)]
