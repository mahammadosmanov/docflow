"""Operation kayit defteri. Yeni ozellik = @register + tek dosya."""
from __future__ import annotations

from .contracts import Operation

_OPS: dict[str, type[Operation]] = {}


def register(op_id: str):
    def deco(cls: type[Operation]) -> type[Operation]:
        if op_id in _OPS:
            raise ValueError(f"duplicate operation id: {op_id}")
        cls.id = op_id
        _OPS[op_id] = cls
        return cls
    return deco


def get(op_id: str) -> Operation:
    if op_id not in _OPS:
        raise KeyError(op_id)
    return _OPS[op_id]()


def all_operations() -> list[Operation]:
    return [cls() for cls in _OPS.values()]
