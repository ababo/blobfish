import argparse
import asyncio
import logging
import segment
import os
import tornado


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        prog='infsrv',
        description='Blobfish Inference Server')
    parser.add_argument('-l', '--log-level', default='INFO')
    parser.add_argument('-a', '--server-address',
                        default=os.environ.get('SERVER_ADDRESS'))
    parser.add_argument('-p', '--server-port',
                        default=os.environ.get('SERVER_PORT', '80'))
    return parser.parse_args()


def make_web_app() -> tornado.web.Application:
    return tornado.web.Application([
        (r"/segment", segment.SegmentHandler),
    ])


async def main():
    args = parse_args()

    logging.basicConfig(level=args.log_level.upper())
    logging.info('starting infsrv')

    app = make_web_app()
    app.listen(args.server_port, args.server_address)

    await asyncio.Event().wait()

if __name__ == "__main__":
    asyncio.run(main())
