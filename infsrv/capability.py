"""Capability definitions."""

from dataclasses import dataclass, field
from typing import Any, Dict, List, Self

from dataclasses_json import dataclass_json, LetterCase

_capability_set: Any = None


@dataclass_json(letter_case=LetterCase.CAMEL)
@dataclass
class Capability:
    """Individual capability metadata."""
    compute_device: str
    model_dirs: List[str]
    model_load_path: str
    module: str

    beam_size: int | None = field(default=None)
    compute_type: str | None = field(default=None)


@dataclass_json(letter_case=LetterCase.CAMEL)
@dataclass
class CapabilitySet:
    """Set of capability metadata."""
    capabilities: Dict[str, Capability]

    @classmethod
    def get(cls) -> Self:
        """Get a singleton instance."""
        global _capability_set  # pylint: disable=global-statement
        if _capability_set is None:
            with open('infsrv/capability.json', encoding='utf-8') as file:
                # pylint: disable=no-member
                _capability_set = CapabilitySet.from_json(file.read())

        return _capability_set

    def module_capabilities(self, module: str) -> List[Capability]:
        """Get capability dictionary filtered by a given module."""
        items = self.capabilities.items()
        return {k: v for k, v in items if v.module == module}
