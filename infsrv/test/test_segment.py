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
    producer = SegmentProducer(100, 5, 150, 2)

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

    segments = producer.next_window([(20, 80)])  # 1100-1200
    assert segments == [Segment(KIND_VOID, 1100, 1120),
                        Segment(KIND_SPEECH, 1120, 1180),
                        Segment(KIND_VOID, 1180, 1200)]

    segments = producer.next_window([])  # 1200-1300
    assert segments == [Segment(KIND_VOID, 1200, 1300)]

    segments = producer.next_window(
        [(20, 30), (50, 99)], last=True)  # 1300-1400
    assert segments == [Segment(KIND_VOID, 1300, 1320),
                        Segment(KIND_SPEECH, 1320, 1330),
                        Segment(KIND_VOID, 1330, 1350),
                        Segment(KIND_SPEECH, 1350, 1400)]


def test_segment_producer_min_speech_duration() -> None:
    """Perform SegmentProducer min_speech_duration test."""
    producer = SegmentProducer(100, 40, 150, 2)

    segments = producer.next_window([(0, 10), (20, 50), (75, 99)])  # 0-100
    assert segments == [Segment(KIND_SPEECH, 0, 50),
                        Segment(KIND_VOID, 50, 75)]

    segments = producer.next_window([(5, 20), (50, 70)])  # 100-200
    assert segments == [Segment(KIND_SPEECH, 75, 120),
                        Segment(KIND_VOID, 120, 150),
                        Segment(KIND_SPEECH, 150, 200)]

    segments = producer.next_window([(80, 90)], last=True)  # 200-300
    assert segments == [Segment(KIND_VOID, 200, 280),
                        Segment(KIND_SPEECH, 280, 300)]
