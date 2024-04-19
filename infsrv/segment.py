import json
from typing import List, Self


class Segment:
    def __init__(self,
                 begin: int,
                 end: int | None = None) -> None:
        self.begin = begin
        self.end = end

    def to_json(self):
        obj = {'begin': self.begin}
        if self.end is not None:
            obj['end'] = self.end
        return json.dumps(obj)


class Segmenter:
    def __init__(self,
                 rolling_window_duration: int,
                 rolling_window_step: int,
                 time_epsilon: int = 100) -> Self:
        self.rolling_window_duration = rolling_window_duration
        self.rolling_window_step = rolling_window_step
        self.time_epsilon = time_epsilon
        self._num_steps = 0
        self._open = False
        self._from = 0

    def add_rolling_window(self, segments: List[Segment]) -> List[Segment]:
        elapsed = (self._num_steps + 1) * self.rolling_window_step
        offset = max(0, elapsed - self.rolling_window_duration)
        limit = min(self.rolling_window_duration,
                    (self._num_steps + 1) * self.rolling_window_step)

        out_segments = []
        for segment in segments:
            open_end = segment.end > limit - self.time_epsilon
            segment.begin += offset
            segment.end += offset

            if self._open:
                if segment.end < self._from:
                    continue
                if open_end:
                    break
                out_segments.append(
                    Segment(begin=self._from, end=segment.end))
                self._from = segment.end
                self._open = False
            else:
                if segment.begin < self._from:
                    continue
                if open_end:
                    out_segments.append(Segment(begin=segment.begin))
                    self._from = segment.begin
                    self._open = True
                else:
                    out_segments.append(
                        Segment(begin=segment.begin, end=segment.end))
                    self._from = segment.end

        self._num_steps += 1
        return out_segments
