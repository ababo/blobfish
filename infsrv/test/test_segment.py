from typing import Callable, List, Tuple

from segment import ChunkDivider, SegmentProducer


def _create_chunk_divider_callback(
        parts: List[bytes]
) -> Tuple[Callable[[bytes], None], Callable[[], None]]:
    index = 0

    def callback(part: bytes) -> None:
        nonlocal index
        assert parts[index] == part
        index += 1

    def assert_consumed() -> bool:
        nonlocal index
        assert index == len(parts)

    return callback, assert_consumed


def test_chunk_divider() -> None:
    parts = [b'abcd', b'efgh', b'ijkl']
    callback, assert_consumed = _create_chunk_divider_callback(parts)

    divider = ChunkDivider(4, callback)
    divider.add(b'abc')
    divider.add(b'def')
    divider.add(b'ghijklmn')

    assert_consumed()


def test_segment_producer() -> None:
    producer = SegmentProducer(100, 2)

    segments = producer.next_window([(0, 10), (20, 50), (75, 99)])
    assert segments == [(0, 10), (20, 50)]

    segments = producer.next_window([(1, 15), (35, 70), (85, 110)])
    assert segments == [(75, 115), (135, 170)]

    segments = producer.next_window([(0, 100)])
    assert segments == []

    segments = producer.next_window([(25, 55)])
    assert segments == [(185, 300), (325, 355)]
