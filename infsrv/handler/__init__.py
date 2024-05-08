"""Request handler utility definitions."""

import asyncio
from concurrent.futures import ThreadPoolExecutor
from http import HTTPStatus
from typing import Any, Callable, Iterable

from tornado.web import RequestHandler

CAPABILITIES_HEADER = 'X-Blobfish-Capabilities'
CONTENT_TYPE_HEADER = 'Content-Type'
CONTENT_TYPE_JSON = 'application/json'

_executor = ThreadPoolExecutor()


def find_request_capability(capabilities: Iterable[str],
                            handler: RequestHandler) -> str | None:
    """
    Find capability from a given list for a given request.
    If no capability found the request fails with Bad Request.
    """
    req_caps = handler.request.headers.get(CAPABILITIES_HEADER)
    req_caps = [] if req_caps is None else req_caps.split(',')

    capabilities = list(capabilities)
    for cap in req_caps:
        if cap in capabilities:
            return cap

    handler.set_status(HTTPStatus.BAD_REQUEST)
    handler.finish(
        'missing, unknown or disabled capability, expected one '
        f"of {capabilities}' in '{CAPABILITIES_HEADER}' header")
    return None


async def run_sync_task(task: Callable[..., Any], *args: ...) -> Any:
    """Run a synchronous task in a thread pool."""
    loop = asyncio.get_event_loop()
    return await loop.run_in_executor(_executor, task, *args)
