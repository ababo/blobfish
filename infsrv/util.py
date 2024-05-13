"""Miscellaneous utilities."""

import logging
import time

_loggers: dict[str, logging.Logger] = {}


LOGGING_DATEFMT = '%Y-%m-%d %H:%M:%S'
LOGGING_FMT = '%(asctime)s.%(msecs)03d %(levelname)s %(message)s'


def add_logger(name: str) -> logging.Logger:
    """Create a setup a new logger."""

    logger = logging.getLogger(name)

    handler = logging.StreamHandler()
    formatter = logging.Formatter(
        datefmt=LOGGING_DATEFMT, fmt=LOGGING_FMT)
    formatter.converter = time.gmtime
    handler.setFormatter(formatter)
    logger.addHandler(handler)
    logger.propagate = False

    _loggers[name] = logger
    return logger


def setup_logging(level: str) -> None:
    """Configure logging and set level."""

    level = level.upper()
    logging.basicConfig(datefmt=LOGGING_DATEFMT,
                        format=LOGGING_FMT, level=level)
    logging.Formatter.converter = time.gmtime
    for logger in _loggers.values():
        logger.setLevel(level)
