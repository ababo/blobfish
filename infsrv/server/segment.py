"""Speech segmentation."""

import asyncio
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from typing import Dict, List, Tuple

from fastapi import (
    Header, HTTPException, Query, WebSocket, WebSocketDisconnect, status,
)
from pyannote.audio import Pipeline
from pyannote.core.annotation import Annotation
import torch


from capability import CapabilitySet
from server.common import (
    CAPABILITIES_HEADER, TERMINATOR_HEADER, find_request_capability
)
from segment import ChunkDivider, SegmentProducer
import util

_SAMPLE_SIZES = {'i16': 2, 'i32': 4, 'f32': 4}
_SAMPLE_DTYPES = {'i16': torch.int16, 'i32': torch.int32, 'f32': torch.float32}

_logger = util.add_logger('server/segment')


@dataclass
class _Context:
    websocket: WebSocket
    num_channels: int
    sample_rate: float
    sample_type: str
    pipeline: Pipeline
    segment_producer: SegmentProducer


class SegmentHandler:  # pylint: disable=too-few-public-methods
    """Handler for speech segmenting requests."""

    def __init__(
            self,
            executor: ThreadPoolExecutor,
            capabilities: List[str]
    ) -> None:
        self._executor = executor

        self._pipelines: Dict[str, Pipeline] = {}
        module_capabilities = CapabilitySet.get(). \
            module_capabilities('server/segment')
        for name, capability in module_capabilities.items():
            if name in capabilities:
                pipeline = Pipeline.from_pretrained(capability.model_load_path)
                pipeline.to(torch.device(capability.compute_device))
                self._pipelines[name] = pipeline

    async def endpoint(
        self,
        websocket: WebSocket,
        min_speech_duration: float = Query(..., alias='minsd'),
        max_segment_duration: float = Query(..., alias='maxsd'),
        num_channels: int = Query(..., alias='nc'),
        sample_rate: float = Query(..., alias='sr'),
        sample_type: str = Query(..., alias='st'),
        window_duration: float = Query(alias='wd', default=5),
        capabilities: str = Header(..., alias=CAPABILITIES_HEADER),
        content_type: str = Header(...),
        terminator: str | None = Header(
            alias=TERMINATOR_HEADER, default=None),
    ) -> None:
        """Speech segmenting endpoint."""
        # pylint: disable=too-many-arguments
        # pylint: disable=too-many-locals,
        # pylint: disable=too-many-return-statements
        await websocket.accept()

        if min_speech_duration < 1 or min_speech_duration > 60:
            await websocket.close(
                status.WS_1002_PROTOCOL_ERROR,
                'missing, malformed or unsupported '
                "'minsd' (min speech duration) query parameter")
            return

        if max_segment_duration < 5 or max_segment_duration > 300:
            await websocket.close(
                status.WS_1002_PROTOCOL_ERROR,
                'missing, malformed or unsupported '
                "'maxsd' (max segment duration) query parameter")
            return

        if min_speech_duration > max_segment_duration:
            await websocket.close(
                status.WS_1002_PROTOCOL_ERROR, "'minsd' greater than 'maxsd'")
            return

        if num_channels < 1 or num_channels > 8:
            await websocket.close(
                status.WS_1002_PROTOCOL_ERROR,
                'missing, malformed or unsupported '
                "'nc' (number of channels) query parameter")
            return

        if sample_rate < 8000 or sample_rate > 192000:
            await websocket.close(
                status.WS_1002_PROTOCOL_ERROR,
                'missing, malformed or unsupported '
                "'sr' (sample rate) query parameter")
            return

        if sample_type not in _SAMPLE_SIZES:
            await websocket.close(
                status.WS_1002_PROTOCOL_ERROR,
                "missing or unknown 'st' (sample type) "
                "query parameter, expected 'i16', 'i32' or 'f32'")
            return

        if window_duration < 1 or window_duration > 10:
            await websocket.close(
                status.WS_1002_PROTOCOL_ERROR,
                "malformed or unsupported 'wd' "
                '(window duration secs) query parameter')
            return

        try:
            capability = find_request_capability(
                self._pipelines.keys(), capabilities)
        except HTTPException as e:
            await websocket.close(status.WS_1002_PROTOCOL_ERROR, e.detail)
            return

        if content_type != 'audio/lpcm':
            await websocket.close(
                status.WS_1008_POLICY_VIOLATION,
                "unsupported audio type, expected 'audio/lpcm'")
            return

        terminator = None if terminator is None \
            else bytes(terminator, encoding='ISO-8859-1')

        segment_producer = SegmentProducer(
            window_duration, min_speech_duration, max_segment_duration, 0.1)
        ctx = _Context(websocket, num_channels, sample_rate, sample_type,
                       self._pipelines[capability], segment_producer)

        window_buffer_len = int(
            window_duration * num_channels *
            sample_rate * _SAMPLE_SIZES[sample_type])
        chunk_divider = ChunkDivider(
            window_buffer_len,
            lambda data, last: self._chunk_divider_callback(ctx, data, last))

        while True:
            try:
                data = await websocket.receive_bytes()
                if terminator is not None and \
                        data[-len(terminator):] == terminator:
                    _logger.debug('detected pcm stream terminator')
                    await chunk_divider.add(data[:-len(terminator)], last=True)
                    await websocket.close()
                    break
                await chunk_divider.add(data)
            except WebSocketDisconnect as err:
                _logger.debug('ws disconnect error: %s', err)
                break

    async def _chunk_divider_callback(
            self, ctx: _Context, data: bytes, last: bool) -> None:
        loop = asyncio.get_event_loop()
        annotation = await loop.run_in_executor(
            self._executor, _annotate_window, ctx, data)

        segments = ctx.segment_producer.next_window(
            _annotation_intervals(annotation), last)

        for segment in segments:
            if segment.end - segment.begin > 0.1:
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


def _annotation_intervals(
    annotation: Annotation
) -> List[Tuple[float, float]]:
    intervals = []
    last_end = 0

    for segment, _ in annotation.itertracks():
        begin = max(segment.start, last_end)
        if segment.end > begin:
            intervals.append((begin, segment.end))
            last_end = segment.end

    return intervals
