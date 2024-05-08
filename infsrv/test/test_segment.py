"""Tests for segmentation logic."""

from typing import Callable, List, Tuple

import pytest

from segment import ChunkDivider, SegmentProducer


def _create_chunk_divider_callback(
        parts: List[bytes]
) -> Tuple[Callable[[bytes], None], Callable[[], None]]:
    index = 0

    async def callback(part: bytes) -> None:
        nonlocal index
        assert parts[index] == part
        index += 1

    def assert_consumed() -> bool:
        nonlocal index
        assert index == len(parts)

    return callback, assert_consumed

@pytest.mark.asyncio
async def test_chunk_divider() -> None:
    """Perform ChunkDivider sanity test."""
    parts = [b'abcd', b'efgh', b'ijkl']
    callback, assert_consumed = _create_chunk_divider_callback(parts)

    divider = ChunkDivider(4, callback)
    await divider.add(b'abc')
    await divider.add(b'def')
    await divider.add(b'ghijklmn')

    assert_consumed()


def test_segment_producer() -> None:
    """Perform SegmentProducer sanity test."""
    producer = SegmentProducer(100, 2)

    segments = producer.next_window([(0, 10), (20, 50), (75, 99)])
    assert segments == [(0, 10), (20, 50)]

    segments = producer.next_window([(1, 15), (35, 70), (85, 110)])
    assert segments == [(75, 115), (135, 170)]

    segments = producer.next_window([(0, 100)])
    assert not segments

    segments = producer.next_window([(25, 55)])
    assert segments == [(185, 300), (325, 355)]
