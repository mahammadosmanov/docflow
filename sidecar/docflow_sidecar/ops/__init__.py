"""Operation'lar. Yeni ozellik = bu klasorde TEK dosya + burada tek import.

Baska hicbir yere dokunma. Dokunman gerekiyorsa mimari bozulmus demektir.
"""
from . import rfi_process_scans  # noqa: F401
from . import rfi_log_add  # noqa: F401

__all__ = ["rfi_process_scans", "rfi_log_add"]
