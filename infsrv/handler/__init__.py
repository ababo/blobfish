"""Request handler utility definitions."""

import asyncio
from concurrent.futures import ThreadPoolExecutor
from http import HTTPStatus
from typing import Any, Callable, Iterable, List

from fastapi import HTTPException

CAPABILITIES_HEADER = 'X-Blobfish-Capabilities'
TERMINATOR_HEADER = 'X-Blobfish-Terminator'

_executor = ThreadPoolExecutor()


def find_request_capability(
        enabled: Iterable[str],
        header: str
) -> str:
    """
    Find capability in a given value of the capabilities header.
    If no capability found the request fails with Bad Request.
    """
    req_caps = header.split(',')

    enabled = list(enabled)
    for cap in req_caps:
        if cap in enabled:
            return cap

    raise HTTPException(
        HTTPStatus.BAD_REQUEST,
        'missing, unknown or disabled capability, expected '
        f"one of {enabled}' in '{CAPABILITIES_HEADER}' header")


async def run_sync_task(task: Callable[..., Any], *args: ...) -> Any:
    """Run a synchronous task in a thread pool."""
    loop = asyncio.get_event_loop()
    return await loop.run_in_executor(_executor, task, *args)
