# IPC protokolü

stdin/stdout üzerinde **satır-ayrımlı JSON**. Her mesaj tek satır, `\n` ile biter.

Neden HTTP değil: localhost dinleyen bir process kurumsal Windows'ta firewall
pop-up'ı yer. stdio bunu tamamen atlar, port çakışması yok, kimlik doğrulama yok.

## Değişmez

**stdout SADECE JSON-RPC içindir.** Tek bir `print()` protokolü bozar ve hata
vermez — istemci sessizce parse hatası alır. Log → stderr, istisnasız.
`rpc.py` başlangıçta stdout'u kilitler; kaza eseri `print()` çağıran kod
`RuntimeError` alır. Bu bir güvenlik ağı, izin değil.

## İstek / yanıt

```jsonc
→ {"id": 1, "method": "ping", "params": {}}
← {"id": 1, "result": {"ok": true, "version": "0.1.0"}}

→ {"id": 2, "method": "list_operations", "params": {}}
← {"id": 2, "result": [{"id": "rfi.process_scans", "titleKey": "...",
                        "descriptionKey": "...", "schema": [...]}]}

→ {"id": 3, "method": "preview",
   "params": {"op": "rfi.process_scans",
              "params": {"source": "/in", "dest": "/out", "keep_original": false}}}
← {"id": 3, "result": {"planId": "uuid", "opId": "rfi.process_scans",
                       "steps": [...], "warnings": [...]}}

→ {"id": 4, "method": "apply", "params": {"planId": "uuid"}}
← {"id": 4, "result": {"applied": 12, "failed": 0, "journalId": "uuid", "errors": []}}

→ {"id": 5, "method": "undo", "params": {"journalId": "uuid"}}
← {"id": 5, "result": {"applied": 12, "failed": 0, "journalId": "uuid", "errors": []}}
```

## Bildirim (tek yönlü, `id` YOK)

```jsonc
← {"method": "progress", "params": {"phase": "preview", "done": 3, "total": 20}}
← {"method": "progress", "params": {"phase": "apply",   "done": 8, "total": 24}}
```

İstemci `id` alanı olmayan mesajı bildirim sayar ve yanıt beklemez.

## Hata

```jsonc
← {"id": 3, "error": {"code": -32000, "message": "KeyError: plan bulunamadı"}}
```

| kod | anlam |
|---|---|
| -32700 | JSON parse hatası |
| -32000 | handler exception attı |

Sidecar **asla ölmez**. Handler exception'ı yakalanır, error olarak döner,
döngü devam eder.

## Adım (Step) şeması

```jsonc
{"kind": "mkdir" | "move" | "copy" | "rename" | "write",
 "src": "kaynak yol veya null",
 "dst": "hedef yol",
 "noteKey": "note.renamed",        // i18n anahtarı — düz metin DEĞİL
 "meta": {"code": "AZE-...", "confidence": 1.0, "method": "strict", "rev": 0}}
```

`meta` serbest — UI gösterebilir, yok sayabilir. `noteKey` her zaman i18n anahtarı.

## Yaşam döngüsü

1. Flutter açılışta sidecar'ı `Process.start` ile başlatır
2. `ping` ile hazır olduğunu doğrular
3. `list_operations` ile UI'ı üretir
4. Sidecar ölürse yeniden başlat. Üst üste 3 kez ölürse `error.sidecar_down` göster.
5. Uygulama kapanırken stdin'i kapat → sidecar döngüden çıkar
