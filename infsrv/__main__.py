from argparse import ArgumentParser, Namespace
import asyncio
import os

from tornado.web import Application

import segment
import util

logger = util.add_logger('infsrv')


def parse_args() -> Namespace:
    parser = ArgumentParser(
        prog='infsrv',
        description='Blobfish Inference Server')
    parser.add_argument('-l', '--log-level', default='INFO')
    parser.add_argument('-a', '--server-address',
                        default=os.environ.get('SERVER_ADDRESS'))
    parser.add_argument('-p', '--server-port',
                        default=os.environ.get('SERVER_PORT', '80'))
    return parser.parse_args()


def make_web_app() -> Application:
    return Application([
        (r"/segment", segment.SegmentHandler),
    ])


async def main() -> None:
    args = parse_args()
    util.setup_logging(args.log_level)
    logger.info(f'starting infsrv with args {vars(args)}')

    app = make_web_app()
    app.listen(args.server_port, args.server_address)

    await asyncio.Event().wait()

if __name__ == "__main__":
    asyncio.run(main())
