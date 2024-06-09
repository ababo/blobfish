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
    parser.add_argument(
        '--header', action='append', nargs=2, metavar=('KEY', 'VALUE'),
        default=[('Content-Type', 'audio/ogg; codecs=vorbis')])
    parser.add_argument('-g', '--language', default=env('LANGUAGE'))
    parser.add_argument('-k', '--access-token', default=env('ACCESS_TOKEN'))
    parser.add_argument('-l', '--log-level', default=env('LOG_LEVEL', 'INFO'))
    parser.add_argument('-p', '--server-port',
                        default=env('SERVER_PORT', '9321'))
    parser.add_argument('-r', '--recording', default=env('RECORDING'))
    parser.add_argument('-s', '--enable-ssl', default=False)
    parser.add_argument('-t', '--tariff', default=env('TARIFF', 'basic'))
    parser.add_argument('--terminator', default=env(
        'TERMINATOR', 'measure-accuracy-terminator'))

    args = parser.parse_args()

    if args.recording is not None and args.language is None:
        parser.error('no language specified for recording')

    return args


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


async def transcribe(
        args: Namespace,
        path: str,
        language: str | None = None
) -> str:
    """Transcribe a given recording path."""
    language = args.language if language is None else language
    address = f'{args.server_address}:{args.server_port}'
    query = f'tariff={args.tariff}&lang={language}'
    proto = 'wss' if args.enable_ssl else 'ws'
    url = f'{proto}://{address}/transcribe?{query}'

    headers = dict(args.header)
    headers['Authorization'] = f'Bearer {args.access_token}'
    headers['X-Blobfish-Terminator'] = args.terminator

    async with connect(url, extra_headers=headers, ping_interval=None) as ws:
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


async def measure_recording_accuracy(
        args: Namespace,
        language: str | None = None,
        recording: str | None = None
) -> float:
    """Measure transcription accuracy for a given recording."""
    language = args.language if language is None else language
    recording = args.recording if recording is None else recording
    prefix = os.path.join(args.data_dir, language, recording)
    txt_path, ogg_file = f'{prefix}.txt', f'{prefix}.ogg'

    with open(txt_path, 'r') as file:
        expected_text = file.read()
        logging.info(
            f'{language}/{recording} reference text:\n\n{expected_text}\n')

    actual_text = await transcribe(args, ogg_file, language)
    logging.info(
        f'{language}/{recording} transcribed text:\n\n{actual_text}\n')

    distance = edit_distance(actual_text, expected_text)
    accuracy = 1 - distance / len(expected_text)
    logging.info(f'{language}/{recording} accuracy is {accuracy}')
    return accuracy


async def measure_language_accuracy(
        args: Namespace,
        language: str | None = None,
) -> float:
    """Measure transcription accuracy for a given language."""
    language = args.language if language is None else language

    index, mean_accuracy = 0, 0
    with os.scandir(os.path.join(args.data_dir, language)) as entries:
        for entry in entries:
            if not entry.is_file() or not entry.path.endswith('.ogg'):
                continue
            recording, _ = os.path.splitext(entry.name)
            accuracy = await measure_recording_accuracy(
                args, language=language, recording=recording)
            mean_accuracy += (accuracy - mean_accuracy) / (index + 1)
            index += 1

    logging.info(f'{language} mean accuracy is {mean_accuracy}')
    return mean_accuracy


async def main():
    args = _parse_args()

    logging.basicConfig(level=args.log_level.upper())

    if args.recording is not None:
        await measure_recording_accuracy(args)
        return

    if args.language is not None:
        accuracy = await measure_language_accuracy(args)
        return

    index, mean_accuracy = 0, 0
    with os.scandir(args.data_dir) as entries:
        for entry in entries:
            if not entry.is_dir():
                continue
            accuracy = await measure_language_accuracy(
                args, language=entry.name)
            mean_accuracy += (accuracy - mean_accuracy) / (index + 1)
            index += 1

    logging.info(f'mean accuracy is {mean_accuracy}')
    print(mean_accuracy)

if __name__ == "__main__":
    asyncio.run(main())
