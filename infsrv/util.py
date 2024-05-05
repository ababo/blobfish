"""Miscellaneous utilities."""

import logging

_loggers: dict[str, logging.Logger] = {}


_LOGGING_DATE_FORMAT = '%Y-%m-%d %H:%M:%S'
_LOGGING_FORMAT = '%(asctime)s.%(msecs)03d %(levelname)s %(message)s'


def add_logger(name: str) -> logging.Logger:
    """Create a setup a new logger."""

    logger = logging.getLogger(name)

    handler = logging.StreamHandler()
    formatter = logging.Formatter(
        datefmt=_LOGGING_DATE_FORMAT, fmt=_LOGGING_FORMAT)
    handler.setFormatter(formatter)
    logger.addHandler(handler)
    logger.propagate = False

    _loggers[name] = logger
    return logger


def setup_logging(level: str) -> None:
    """Configure logging and set level."""

    level = level.upper()
    logging.basicConfig(datefmt=_LOGGING_DATE_FORMAT,
                        format=_LOGGING_FORMAT, level=level)
    for logger in _loggers.values():
        logger.setLevel(level)
