# DocFlow

Masaüstü belge otomasyon uygulaması. Flutter (UI) + Python sidecar (iş mantığı).

MVP: taranmış RFI formlarını ilk sayfadaki RFI numarasına göre adlandırır ve klasörler.

## Başlarken

```bash
# 0. Tesseract
brew install tesseract                    # macOS
sudo pacman -S tesseract tesseract-data-eng   # Arch / CachyOS

# 1. Sidecar
cd sidecar
python -m venv .venv && source .venv/bin/activate
pip install -r requirements-dev.txt
python -m pytest -q                       # 15 passed

# 2. ÖNCE KALİBRE ET — bkz. docs/TASKS.md Görev 0.1
python ../tools/rfi_probe.py /gercek/taramalar --sweep

# 3. Flutter — henüz yok, docs/TASKS.md Faz 2
```

## Okuma sırası

1. `CLAUDE.md` — kurallar ve ölçülmüş OCR gerçekleri
2. `docs/ARCHITECTURE.md` — neden bu şekilde
3. `docs/TASKS.md` — ne yapılacak, hangi sırayla
4. `docs/IPC_PROTOCOL.md` — Flutter ↔ Python sözleşmesi

## Durum

| Parça | Durum |
|---|---|
| Sözleşmeler (`contracts.py`) | ✅ |
| RFI çıkarımı (`extract/`) | ✅ gerçek belge üzerinde test edildi |
| Uygulama + journal (`fs/apply.py`) | ✅ |
| JSON-RPC (`rpc.py`) | ✅ |
| Referans operation | ✅ |
| DPI kalibrasyonu | ⬜ gerçek taramalar gerekli |
| Flutter | ⬜ |
| Paketleme | ⬜ |
