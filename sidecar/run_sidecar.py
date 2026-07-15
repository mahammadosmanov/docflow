"""PyInstaller giris noktasi. Frozen exe stdio JSON-RPC dongusunu calistirir.

Dev'de sidecar `python -m docflow_sidecar` ile calisir; paketlenmis Windows
derlemesinde bu dosya PyInstaller ile `docflow_sidecar.exe`e donusur.
"""
from docflow_sidecar.__main__ import main

if __name__ == "__main__":
    main()
