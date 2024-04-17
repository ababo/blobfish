import logging
from typing import Any

loggers: dict[str, logging.Logger] = {}


LOGGING_DATE_FORMAT = '%Y-%m-%d %H:%M:%S'
LOGGING_FORMAT = '%(asctime)s.%(msecs)03d %(levelname)s %(message)s'


def add_logger(name: str) -> logging.Logger:
    logger = logging.getLogger(name)

    handler = logging.StreamHandler()
    formatter = logging.Formatter(
        datefmt=LOGGING_DATE_FORMAT, fmt=LOGGING_FORMAT)
    handler.setFormatter(formatter)
    logger.addHandler(handler)
    logger.propagate = False

    loggers[name] = logger
    return logger


def setup_logging(level: str) -> None:
    level = level.upper()
    logging.basicConfig(datefmt=LOGGING_DATE_FORMAT,
                        format=LOGGING_FORMAT, level=level)
    for logger in loggers.values():
        logger.setLevel(level)


class RingBuffer:
    def __init__(self, capacity) -> Any:
        self._data = bytearray(capacity)
        self._empty = True
        self._from = 0
        self._to = 0

    def __len__(self) -> int:
        if not self._empty and self._to == self._from:
            return len(self._data)
        return (self._to - self._from) % len(self._data)

    def add(self, data: bytearray | bytes) -> Any:
        data = data[-min(len(data), len(self._data)):]
        split = min(len(data), len(self._data) - self._to)
        self._data[self._to:self._to+split] = data[:split]
        self._data[:len(data)-split] = data[split:]
        from_inc = max(0, len(data) - len(self._data) + len(self))
        self._from = (self._from + from_inc) % len(self._data)
        self._to = (self._to + len(data)) % len(self._data)
        self._empty = self._empty and len(data) == 0

    def data(self) -> bytearray:
        return (self._data[self._from:] + self._data[:self._from])[:len(self)]
