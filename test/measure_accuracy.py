#!/usr/bin/env python3

from argparse import ArgumentParser, Namespace
import asyncio
import json
import logging
import os
import os.path

from nltk import edit_distance
from websockets import ConnectionClosedError, ConnectionClosedOK
from websockets.client import connect, ClientConnection


def _parse_args() -> Namespace:
    env = os.environ.get

    parser = ArgumentParser(
        prog='infsrv',
        description='Blobfish Accuracy Test')

    parser.add_argument('-a', '--server-address',
                        default=env('SERVER_ADDRESS', '127.0.0.1'))
    parser.add_argument('-d', '--data-dir',
                        default=env('DATA_DIR', 'test/data'))
    parser.add_argument('-r', '--recording_path',
                        default=env('RECORDING_PATH'))
    parser.add_argument(
        '--header', action='append', nargs=2, metavar=('KEY', 'VALUE'),
        default=[('Content-Type', 'audio/ogg; codecs=vorbis')])
    parser.add_argument('-l', '--log-level', default=env('LOG_LEVEL', 'INFO'))
    parser.add_argument('-p', '--server-port',
                        default=env('SERVER_PORT', '9321'))
    parser.add_argument('-t', '--tariff', default=env('TARIFF', 'basic'))
    parser.add_argument('--terminator', default=env(
        'TERMINATOR', 'measure-accuracy-terminator'))

    return parser.parse_args()


async def _read_segments(ws: ClientConnection) -> str:
    text = ''
    while True:
        try:
            item_json = await ws.recv()
        except ConnectionClosedError as err:
            logging.debug(f'connection closed with error {err}')
            break
        except ConnectionClosedOK:
            logging.debug('connection closed gracefully')
            break

        item = json.loads(item_json)
        text += item['text']
    return text


async def transcribe(args: Namespace, path: str, language: str) -> str:
    """Transcription a given recording file."""
    address = f'{args.server_address}:{args.server_port}'
    query = f'tariff={args.tariff}&lang={language}'
    url = f'ws://{address}/transcribe?{query}'

    headers = dict(args.header)
    headers['X-Blobfish-Terminator'] = args.terminator

    async with connect(url, extra_headers=headers) as ws:
        read_task = asyncio.create_task(_read_segments(ws))

        with open(path, 'rb') as file:
            while True:
                chunk = file.read(8192)
                if not chunk:
                    break
                await ws.send(chunk)
        await ws.send(bytes(args.terminator, encoding='ISO-8859-1'))

        text = await read_task
    return text


async def measure_accuracy(args: Namespace, path: str) -> float:
    """Measure transcription accuracy for a given recording file."""
    name = os.path.basename(path)
    parts = os.path.splitext(name)
    language, _ = parts[0].split('-', 1)

    actual_text = await transcribe(args, path, language)
    logging.debug(f'transcribed text: "{actual_text}"')

    txt_base, _ = os.path.splitext(path)
    txt_path = txt_base + '.txt'
    with open(txt_path, 'r') as file:
        expected_text = file.read()
        logging.debug(f'expected text: "{expected_text}"')

    distance = edit_distance(actual_text, expected_text)
    accuracy = 1 - distance / len(expected_text)
    logging.info(f'{parts[0]} accuracy is {accuracy}')
    return accuracy


async def main():
    args = _parse_args()

    logging.basicConfig(level=args.log_level.upper())

    if args.recording_path is not None:
        accuracy = await measure_accuracy(args, args.recording_path)
        if accuracy is None:
            logging.error('skipped unsupported recording')
        return

    mean_accuracy = 0
    with os.scandir(args.data_dir) as entries:
        for index, entry in enumerate(entries):
            if not entry.is_file():
                continue

            _, extension = os.path.splitext(entry.path)
            if extension != '.ogg':
                continue

            accuracy = await measure_accuracy(args, entry.path)
            mean_accuracy += (accuracy - mean_accuracy) / (index + 1)

        logging.info(f'mean accuracy is {mean_accuracy}')

if __name__ == "__main__":
    asyncio.run(main())
