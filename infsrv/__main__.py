"""Inference server entry point."""

from argparse import ArgumentError, ArgumentParser, Namespace
import asyncio
import os
from typing import Any, List

from fastapi import FastAPI
import uvicorn
import uvicorn.config

from handler import segment, transcribe
from capability import CapabilitySet
import util

_logger = util.add_logger('infsrv')


def _parse_args() -> Namespace:
    env = os.environ.get

    parser = ArgumentParser(
        prog='infsrv',
        description='Blobfish Inference Server')

    def capability_list(value: str) -> List[str]:
        items = value.split(',')
        for item in items:
            if item not in CapabilitySet.get().capabilities.keys():
                raise ArgumentError(
                    'capabilities', f"unknown capability '{item}'")
        return items

    parser.add_argument('-l', '--log-level',
                        default=env('LOG_LEVEL', 'INFO'))
    parser.add_argument('-c', '--capabilities', type=capability_list,
                        default=env('CAPABILITIES', 'pyannote30'))
    parser.add_argument('-a', '--server-address',
                        default=env('SERVER_ADDRESS', '127.0.0.1'))
    parser.add_argument('-p', '--server-port',
                        default=env('SERVER_PORT', '9322'))

    return parser.parse_args()


def _make_web_app() -> FastAPI:
    app = FastAPI()
    app.add_api_websocket_route(
        '/segment',
        segment.handle_segment)
    app.add_api_route(
        '/transcribe',
        transcribe.handle_transcribe,
        methods=['POST'])
    return app


def _create_uvicorn_log_config() -> Any:
    log_config = uvicorn.config.LOGGING_CONFIG
    log_formatter = log_config['formatters']['default']
    log_formatter['fmt'] = util.LOGGING_FMT
    log_formatter['datefmt'] = util.LOGGING_DATEFMT
    log_formatter = log_config['formatters']['access']
    log_formatter['fmt'] = util.LOGGING_FMT
    log_formatter['datefmt'] = util.LOGGING_DATEFMT
    return log_config


async def main() -> None:
    """Inference server logic."""

    args = _parse_args()
    util.setup_logging(args.log_level)
    _logger.info('starting infsrv with args %s', vars(args))

    segment.init(args.capabilities)
    transcribe.init(args.capabilities)
    _logger.info('initialized modules')

    app = _make_web_app()
    loop = asyncio.get_event_loop()
    log_config = _create_uvicorn_log_config()
    config = uvicorn.Config(app=app, loop=loop,
                            host=args.server_address,
                            port=int(args.server_port),
                            log_config=log_config)
    server = uvicorn.Server(config)
    await server.serve()


if __name__ == "__main__":
    asyncio.run(main())
