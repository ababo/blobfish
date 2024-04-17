from util import RingBuffer


def test_ring_buffer():
    buffer = RingBuffer(10)
    assert buffer.data() == b''
    assert len(buffer) == 0

    buffer.add(b'abcdefgh')
    assert buffer.data() == b'abcdefgh'
    assert len(buffer) == 8

    buffer.add(b'ijkl')
    assert buffer.data() == b'cdefghijkl'
    assert len(buffer) == 10

    buffer.add(b'mnopqrst')
    assert buffer.data() == b'klmnopqrst'
    assert len(buffer) == 10

    buffer.add(b'uvwxyz')
    assert buffer.data() == b'qrstuvwxyz'
    assert len(buffer) == 10

    buffer.add(b'abcdefghijklmnopqrstuvwxyz')
    assert buffer.data() == b'qrstuvwxyz'
    assert len(buffer) == 10
