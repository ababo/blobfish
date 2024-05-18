"""Common server definitions."""

from typing import Iterable

from fastapi import HTTPException, status

CAPABILITIES_HEADER = 'X-Blobfish-Capabilities'
TERMINATOR_HEADER = 'X-Blobfish-Terminator'


def find_request_capability(
        enabled: Iterable[str],
        header: str,
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
        status.HTTP_400_BAD_REQUEST,
        'missing, unknown or disabled capability, expected '
        f"one of {enabled}' in '{CAPABILITIES_HEADER}' header")
