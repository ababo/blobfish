from segment import Segment, Segmenter


def assert_segments_equal(actual, expected):
    assert len(actual) == len(expected)
    for i in range(len(actual)):
        assert actual[i].begin == expected[i].begin
        assert actual[i].end == expected[i].end


def test_continuous():
    segmenter = Segmenter(
        rolling_window_duration=5000, rolling_window_step=1000)

    segments = [  # filled 1000ms, offset 0ms
        Segment(begin=8, end=1129),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [
        Segment(begin=8),
    ])

    segments = [  # filled 2000ms, offset 0ms
        Segment(begin=8, end=2028),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [])

    segments = [  # filled 3000ms, offset 0ms
        Segment(begin=8, end=3030),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [])

    segments = [  # filled 4000ms, offset 0ms
        Segment(begin=8, end=4066),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [])

    segments = [  # filled 5000ms, offset 0ms
        Segment(begin=8, end=5084),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [])

    segments = [  # filled 5000ms, offset 1000ms
        Segment(begin=8, end=5101),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [])

    segments = [  # filled 5000ms, offset 2000ms
        Segment(begin=8, end=5016),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [])

    segments = [  # filled 5000ms, offset 3000ms
        Segment(begin=8, end=5050),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [])

    segments = [  # filled 5000ms, offset 4000ms
        Segment(begin=8, end=5101),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [])

    segments = [  # filled 5000ms, offset 5000ms
        Segment(begin=8, end=4185),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [
        Segment(begin=8, end=9185),
    ])

    segments = [  # filled 5000ms, offset 6000ms
        Segment(begin=8, end=3234),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [])


def test_discussion():
    segmenter = Segmenter(
        rolling_window_duration=5000, rolling_window_step=1000)

    # first 25 secs of https://www.youtube.com/watch?v=aSqLS8s2B4c
    segments = [  # filled 1000ms, offset 0ms
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [])

    segments = [  # filled 2000ms, offset 0ms
        Segment(begin=908, end=2113),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [
        Segment(begin=908),
    ])

    segments = [  # filled 3000ms, offset 0ms
        Segment(begin=1044, end=2249),
        Segment(begin=2521, end=3098),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [
        Segment(begin=908, end=2249),
        Segment(begin=2521),
    ])

    segments = [  # filled 4000ms, offset 0ms
        Segment(begin=2521, end=3149),
        Segment(begin=3217, end=4066),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [
        Segment(begin=2521, end=3149),
        Segment(begin=3217),
    ])

    segments = [  # filled 5000ms, offset 0ms
        Segment(begin=1078, end=3166),
        Segment(begin=3217, end=4983),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [])

    segments = [  # filled 5000ms, offset 1000ms
        Segment(begin=59, end=3998),  # ignore inconsistency
        Segment(begin=4168, end=4983),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [
        Segment(begin=3217, end=4998),
        Segment(begin=5168),
    ])

    segments = [  # filled 5000ms, offset 2000ms
        Segment(begin=534, end=1162),
        Segment(begin=1213, end=2962),
        Segment(begin=3183, end=4032),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [
        Segment(begin=5168, end=6032),
    ])

    segments = [  # filled 5000ms, offset 3000ms
        Segment(begin=212, end=1943),
        Segment(begin=2181, end=2979),
        Segment(begin=4168, end=4422),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [
        Segment(begin=7168, end=7422),
    ])

    segments = [  # filled 5000ms, offset 4000ms
        Segment(begin=8, end=1010),
        Segment(begin=1078, end=2011),
        Segment(begin=3234, end=3285),
        Segment(begin=3930, end=4983),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [
        Segment(begin=7930),
    ])

    segments = [  # filled 5000ms, offset 5000ms
        Segment(begin=195, end=1044),
        Segment(begin=2164, end=2385),
        Segment(begin=3030, end=4439),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [
        Segment(begin=7930, end=9439),
    ])

    segments = [  # filled 5000ms, offset 6000ms
        Segment(begin=1179, end=1366),
        Segment(begin=2045, end=3098),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [])

    segments = [  # filled 5000ms, offset 7000ms
        Segment(begin=144, end=331),
        Segment(begin=1010, end=2436),
        Segment(begin=4541, end=5084),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [
        Segment(begin=11541),
    ])

    segments = [  # filled 5000ms, offset 8000ms
        Segment(begin=8, end=1179),
        Segment(begin=2215, end=2249),
        Segment(begin=3471, end=5067),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [])

    segments = [  # filled 5000ms, offset 9000ms
        Segment(begin=8, end=314),
        Segment(begin=2453, end=5033),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [])

    segments = [  # filled 5000ms, offset 10000ms
        Segment(begin=1485, end=3964),
        Segment(begin=4660, end=5000),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [
        Segment(begin=11541, end=13964),
        Segment(begin=14660),
    ])

    segments = [  # filled 5000ms, offset 11000ms
        Segment(begin=517, end=2979),
        Segment(begin=3709, end=5050),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [])

    segments = [  # filled 5000ms, offset 12000ms
        Segment(begin=8, end=1994),
        Segment(begin=2724, end=5000),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [])

    segments = [  # filled 5000ms, offset 13000ms
        Segment(begin=8, end=1010),
        Segment(begin=1672, end=5000),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [])

    segments = [  # filled 5000ms, offset 14000ms
        Segment(begin=687, end=4134),
        Segment(begin=4371, end=4966),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [
        Segment(begin=14660, end=18134),
        Segment(begin=18371),
    ])

    segments = [  # filled 5000ms, offset 15000ms
        Segment(begin=8, end=5033),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [])

    segments = [  # filled 5000ms, offset 16000ms
        Segment(begin=8, end=2062),
        Segment(begin=2385, end=5000),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [])

    segments = [  # filled 5000ms, offset 17000ms
        Segment(begin=8, end=1010),
        Segment(begin=1400, end=3064),
        Segment(begin=3387, end=5016),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [
        Segment(begin=18371, end=20064),
        Segment(begin=20387),
    ])

    segments = [  # filled 5000ms, offset 18000ms
        Segment(begin=415, end=4983),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [])

    segments = [  # filled 5000ms, offset 19000ms
        Segment(begin=8, end=4932),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [])

    segments = [  # filled 5000ms, offset 20000ms
        Segment(begin=365, end=4932),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [])

    segments = [  # filled 5000ms, offset 21000ms
        Segment(begin=8, end=483),
        Segment(begin=687, end=3879),
        Segment(begin=4592, end=5050),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [
        Segment(begin=20387, end=21483),
        Segment(begin=21687, end=24879),
        Segment(begin=25592),
    ])

    segments = [  # filled 5000ms, offset 22000ms
        Segment(begin=8, end=2809),
        Segment(begin=3556, end=4660),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [
        Segment(begin=25592, end=26660)
    ])

    segments = [  # filled 5000ms, offset 23000ms
        Segment(begin=8, end=1876),
        Segment(begin=2555, end=3607),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [])

    segments = [  # filled 5000ms, offset 23000ms
        Segment(begin=8, end=993),
        Segment(begin=1536, end=2589),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [])

    segments = [  # filled 5000ms, offset 24000ms
        Segment(begin=8, end=25),
        Segment(begin=500, end=1570),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [])

    segments = [  # filled 5000ms, offset 25000ms
        Segment(begin=8, end=314),
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [])

    segments = [  # filled 5000ms, offset 26000ms
    ]
    segments = segmenter.add_rolling_window(segments)
    assert_segments_equal(segments, [])
