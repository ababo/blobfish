"""Speech segmentation handler."""

from dataclasses import dataclass
from http import HTTPStatus
from typing import Dict, List

from fastapi import (
    Header, HTTPException, Query, WebSocket, WebSocketDisconnect
)
from pyannote.audio import Pipeline
from pyannote.core.annotation import Annotation
import torch


from capability import CapabilitySet
from handler import (
    CAPABILITIES_HEADER, find_request_capability, run_sync_task
)
from segment import ChunkDivider, SegmentProducer
import util

_SAMPLE_SIZES = {'i16': 2, 'i32': 4, 'f32': 4}
_SAMPLE_DTYPES = {'i16': torch.int16, 'i32': torch.int32, 'f32': torch.float32}

_logger = util.add_logger('segment')

_pyannote_pipelines: Dict[str, Pipeline] = {}


def init(capabilities: List[str]) -> None:
    """Create pyannote pipelines."""
    module_capabilities = CapabilitySet.get(). \
        module_capabilities('handler/segment')
    for name, capability in module_capabilities.items():
        if name in capabilities:
            pipeline = Pipeline.from_pretrained(capability.model_load_path)
            pipeline.to(torch.device(capability.compute_device))
            _pyannote_pipelines[name] = pipeline


@dataclass
class _Context:
    websocket: WebSocket
    num_channels: int
    sample_rate: float
    sample_type: str
    pipeline: Pipeline
    segment_producer: SegmentProducer


async def handle_segment(  # pylint: disable=too-many-arguments
        websocket: WebSocket,
        max_speech_duration: float = Query(..., alias='msd'),
        num_channels: int = Query(..., alias='nc'),
        sample_rate: float = Query(..., alias='sr'),
        sample_type: str = Query(..., alias='st'),
        window_duration: float = Query(alias='wd', default=5),
        capabilities: str = Header(..., alias=CAPABILITIES_HEADER),
        content_type: str = Header(...),
) -> None:
    """Websocket handler for realtime audio segmentation."""
    if max_speech_duration < 10 or max_speech_duration > 90:
        raise HTTPException(
            HTTPStatus.BAD_REQUEST,
            'missing, malformed or unsupported '
            "'msd' (max speech duration) query parameter")

    if num_channels < 1 or num_channels > 8:
        raise HTTPException(
            HTTPStatus.BAD_REQUEST,
            'missing, malformed or unsupported '
            "'nc' (number of channels) query parameter")

    if sample_rate < 8000 or sample_rate > 192000:
        raise HTTPException(
            HTTPStatus.BAD_REQUEST,
            'missing, malformed or unsupported '
            "'sr' (sample rate) query parameter")

    if sample_type not in _SAMPLE_SIZES:
        raise HTTPException(
            HTTPStatus.BAD_REQUEST,
            "missing or unknown 'st' (sample type) "
            "query parameter, expected 'i16', 'i32' or 'f32'")

    if window_duration < 1 or window_duration > 10:
        raise HTTPException(
            HTTPStatus.BAD_REQUEST,
            "malformed or unsupported 'wd' "
            '(window duration secs) query parameter')

    capability = find_request_capability(
        _pyannote_pipelines.keys(), capabilities)
    pipeline = _pyannote_pipelines[capability]

    if content_type != 'audio/lpcm':
        raise HTTPException(
            HTTPStatus.BAD_REQUEST,
            "unsupported audio type, expected 'audio/lpcm'")

    await websocket.accept()
    _logger.debug('open /segment')

    segment_producer = SegmentProducer(
        window_duration, 0.1, max_speech_duration)
    ctx = _Context(websocket, num_channels, sample_rate,
                   sample_type, pipeline, segment_producer)

    window_buffer_len = int(
        window_duration * num_channels *
        sample_rate * _SAMPLE_SIZES[sample_type])
    chunk_divider = ChunkDivider(
        window_buffer_len,
        lambda data: _chunk_divider_callback(ctx, data))

    while True:
        try:
            data = await websocket.receive_bytes()
            await chunk_divider.add(data)
        except WebSocketDisconnect:
            _logger.debug('close /segment')
            break


async def _chunk_divider_callback(ctx: _Context, data: bytes) -> None:
    annotation = await run_sync_task(_annotate_window, ctx, data)

    segments = ctx.segment_producer.next_window(
        map(lambda t: (t[0].start, t[0].end), annotation.itertracks()))

    for segment in segments:
        _logger.debug('sent %s segment %fs-%fs',
                      segment.kind, segment.begin, segment.end)
        await ctx.websocket.send_text(segment.to_json() + '\n')


def _annotate_window(ctx: _Context, data: bytes) -> Annotation:
    dtype = _SAMPLE_DTYPES[ctx.sample_type]
    device = ctx.pipeline.device
    waveform = torch.frombuffer(data, dtype=dtype).to(device)
    waveform = waveform.reshape((-1, ctx.num_channels))
    waveform = torch.transpose(waveform, 0, 1)
    waveform = torch.mean(waveform.float(), dim=0, keepdim=True)
    if not dtype.is_floating_point:
        sample_size = _SAMPLE_SIZES[ctx.sample_type]
        waveform /= 2 ** (sample_size * 8 - 1) - 1
    audio = {'waveform': waveform, 'sample_rate': ctx.sample_rate}
    return ctx.pipeline(audio)
