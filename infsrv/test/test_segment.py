from segment import CircularBuffer

def test_circular_buffer():
    buffer = CircularBuffer(3)
    assert buffer.data() == b''
    assert len(buffer) == 0

    buffer.add(b'ab')
    assert buffer.data() == b'ab'
    assert len(buffer) == 2

    buffer.add(b'cd')
    assert buffer.data() == b'bcd'
    assert len(buffer) == 3

    buffer.add(b'efg')
    assert buffer.data() == b'efg'
    assert len(buffer) == 3
