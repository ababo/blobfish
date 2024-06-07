"""HTTP server."""

import asyncio
from concurrent.futures import ThreadPoolExecutor
from copy import deepcopy
from typing import Any, List

from fastapi import FastAPI
import uvicorn
import uvicorn.config

from server.segment import SegmentHandler
from server.transcribe import TranscribeHandler
import util


class Server:  # pylint: disable=too-few-public-methods
    """HTTP server for inference requests."""

    def __init__(self, capabilities: List[str]) -> None:
        executor = ThreadPoolExecutor()
        self._segment_handler = SegmentHandler(executor, capabilities)
        self._transcribe_handler = TranscribeHandler(executor, capabilities)

        app = FastAPI()
        app.add_api_websocket_route(
            '/segment',
            self._segment_handler.endpoint)
        app.add_api_route(
            '/transcribe',
            self._transcribe_handler.endpoint,
            methods=['POST'])
        self._app = app

    async def serve(self, address: str, port: int) -> None:
        """Serve HTTP requests."""
        loop = asyncio.get_event_loop()
        log_config = _create_uvicorn_log_config()
        config = uvicorn.Config(app=self._app, loop=loop,
                                host=address, port=port,
                                log_config=log_config,
                                ws_ping_interval=None)
        return await uvicorn.Server(config).serve()


def _create_uvicorn_log_config() -> Any:
    log_config = deepcopy(uvicorn.config.LOGGING_CONFIG)
    log_formatter = log_config['formatters']['default']
    log_formatter['fmt'] = util.LOGGING_FMT
    log_formatter['datefmt'] = util.LOGGING_DATEFMT
    log_formatter = log_config['formatters']['access']
    log_formatter['fmt'] = util.LOGGING_FMT
    log_formatter['datefmt'] = util.LOGGING_DATEFMT
    return log_config
