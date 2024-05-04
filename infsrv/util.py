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
