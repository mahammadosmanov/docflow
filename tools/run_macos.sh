#!/usr/bin/env bash
# macOS'ta derle, imzala, calistir.
#
# NEDEN BU SCRIPT VAR: proje iCloud Drive ile senkronlanan Desktop altinda.
# iCloud'un fileprovider'i .framework klasorlerine com.apple.FinderInfo xattr'i
# yapistiriyor ve `xattr -cr` bunu SILEMIYOR (dosya saglayici geri koyuyor).
# codesign bu xattr'i gorunce "resource fork, Finder information, or similar
# detritus not allowed" deyip patliyor — yani `flutter build macos` her zaman
# CodeSign adiminda basarisiz oluyor. Derleme ADIMLARI bitmis oluyor, sadece
# imza kalmis oluyor.
#
# Cozum: bundle'i iCloud DISINA, xattr'lari soyarak kopyala (ditto --noextattr)
# ve orada imzala. Uygulama oradan calisir.
set -uo pipefail

APP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../app" && pwd)"
OUT="${DOCFLOW_RUN_DIR:-/tmp/docflow-run}"
SRC="$APP_DIR/build/macos/Build/Products/Debug/docflow.app"
DST="$OUT/docflow.app"

cd "$APP_DIR"

echo "==> flutter build macos --debug"
# CodeSign hatasi BEKLENIYOR (yukariya bak); .app yine de uretiliyor.
flutter build macos --debug 2>&1 | tail -3 || true

if [[ ! -d "$SRC" ]]; then
  echo "HATA: bundle uretilmedi: $SRC" >&2
  echo "Bu gercek bir derleme hatasi — yukaridaki ciktiya bak." >&2
  exit 1
fi

echo "==> iCloud disina kopyala + xattr soy: $DST"
mkdir -p "$OUT"
rm -rf "$DST"
ditto --norsrc --noextattr --noqtn "$SRC" "$DST" || exit 1

echo "==> ad-hoc imzala"
codesign --force --deep --sign - "$DST" 2>&1 | grep -v "replacing existing signature" || true
if ! codesign --verify --deep --strict "$DST"; then
  echo "HATA: imza dogrulanmadi" >&2
  exit 1
fi

echo "==> calistir"
pkill -f "$OUT/docflow.app/Contents/MacOS/docflow" 2>/dev/null || true
open "$DST"
echo "OK: $DST"
