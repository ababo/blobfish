"""Inference server entry point."""

from argparse import ArgumentError, ArgumentParser, Namespace
import asyncio
import os
from typing import List

from tornado.web import Application

from handler import segment
from handler.segment import SegmentHandler
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
                        default=env('SERVER_PORT', '80'))

    return parser.parse_args()


def _make_web_app() -> Application:
    return Application([
        (r"/segment", SegmentHandler),
    ])


async def main() -> None:
    """Inference server logic."""

    args = _parse_args()
    util.setup_logging(args.log_level)
    _logger.info('starting infsrv with args %s', vars(args))

    segment.init(args.capabilities)
    _logger.info('initialized modules')

    app = _make_web_app()
    app.listen(args.server_port, args.server_address)

    await asyncio.Event().wait()

if __name__ == "__main__":
    asyncio.run(main())
