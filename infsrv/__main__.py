"""Inference server."""

from argparse import ArgumentError, ArgumentParser, Namespace
import asyncio
import os
from typing import List

from capability import CapabilitySet
from server import Server
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
                        default=env('SERVER_PORT', '9322'), type=int)

    return parser.parse_args()


async def main() -> None:
    """Run inference server."""

    args = _parse_args()
    util.setup_logging(args.log_level)
    _logger.info('starting infsrv with args %s', vars(args))

    server = Server(args.capabilities)
    _logger.info('created server')
    await server.serve(args.server_address, args.server_port)


if __name__ == "__main__":
    asyncio.run(main())
