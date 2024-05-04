from argparse import ArgumentParser, Namespace
import asyncio
import os

import torch
from tornado.web import Application

from handler.segment import load_pyannote, SegmentHandler
import util

logger = util.add_logger('infsrv')


def parse_args() -> Namespace:
    env = os.environ.get

    parser = ArgumentParser(
        prog='infsrv',
        description='Blobfish Inference Server')

    parser.add_argument('-l', '--log-level',
                        default=env('LOG_LEVEL', 'INFO'))
    parser.add_argument('--pyannote-model',
                        default=env('PYANNOTE_MODEL',
                                    'model/pyannote/config-3.0.yaml'))
    parser.add_argument('-a', '--server-address',
                        default=env('SERVER_ADDRESS', '127.0.0.1'))
    parser.add_argument('-p', '--server-port',
                        default=env('SERVER_PORT', '80'))
    parser.add_argument('--torch-device', default=env('TORCH_DEVICE', 'cpu'))

    return parser.parse_args()


def make_web_app() -> Application:
    return Application([
        (r"/segment", SegmentHandler),
    ])


async def main() -> None:
    args = parse_args()
    util.setup_logging(args.log_level)
    logger.info(f'starting infsrv with args {vars(args)}')

    torch.set_default_device(args.torch_device)
    load_pyannote(args.pyannote_model, args.torch_device)
    logger.info(f'loaded models')

    app = make_web_app()
    app.listen(args.server_port, args.server_address)

    await asyncio.Event().wait()

if __name__ == "__main__":
    asyncio.run(main())
