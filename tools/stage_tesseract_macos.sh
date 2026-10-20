#!/bin/zsh
# Stage a relocatable, auditable macOS Tesseract resource tree for Tauri.
set -euo pipefail

script_dir=${0:A:h}
project_root=${script_dir:h}
destination="$project_root/src-tauri/resources/tesseract"
requested=${1:-${DOCFLOW_TESSERACT:-/opt/homebrew/bin/tesseract}}

for command_name in otool install_name_tool codesign shasum realpath; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    print -u2 "Required macOS tool is unavailable: $command_name"
    exit 1
  fi
done
if [[ ! -f "$requested" || ! -x "$requested" ]]; then
  print -u2 "Tesseract executable is unavailable: $requested"
  exit 1
fi

staging=$(mktemp -d "${TMPDIR:-/private/tmp}/docflow-tesseract-stage.XXXXXX")
cleanup() {
  if [[ -n ${staging:-} && -d "$staging" && ${staging:t} == docflow-tesseract-stage.* ]]; then
    rm -rf -- "$staging"
  fi
}
trap cleanup EXIT INT TERM
mkdir -p "$staging/lib" "$staging/tessdata" "$staging/licenses"

main_source=$(realpath "$requested")
cp -L "$main_source" "$staging/tesseract"

find_tessdata() {
  local executable=$1
  local directory=${executable:h}
  local parent=${directory:h}
  local candidate
  for candidate in \
    "${DOCFLOW_TESSDATA:-}" \
    "$directory/tessdata" \
    "$parent/tessdata" \
    "$parent/share/tessdata" \
    "$parent/share/tesseract-ocr/5/tessdata" \
    "$parent/share/tesseract-ocr/4.00/tessdata"; do
    if [[ -n "$candidate" && -f "$candidate/eng.traineddata" && -f "$candidate/osd.traineddata" ]]; then
      print -r -- "$candidate"
      return 0
    fi
  done
  return 1
}

tessdata_source=$(find_tessdata "$main_source") || {
  print -u2 "eng.traineddata and osd.traineddata were not found for $requested"
  exit 1
}
cp -L "$tessdata_source/eng.traineddata" "$staging/tessdata/eng.traineddata"
cp -L "$tessdata_source/osd.traineddata" "$staging/tessdata/osd.traineddata"

dependencies() {
  otool -L "$1" | tail -n +2 | awk '{print $1}'
}

resolve_dependency() {
  local source=$1
  local dependency=$2
  local source_dir=${source:h}
  local suffix candidate rpath
  case "$dependency" in
    /*)
      [[ -e "$dependency" ]] && realpath "$dependency"
      ;;
    @loader_path/*)
      suffix=${dependency#@loader_path/}
      candidate="$source_dir/$suffix"
      [[ -e "$candidate" ]] && realpath "$candidate"
      ;;
    @rpath/*)
      suffix=${dependency#@rpath/}
      while IFS= read -r rpath; do
        rpath=${rpath//@loader_path/$source_dir}
        rpath=${rpath//@executable_path/${main_source:h}}
        candidate="$rpath/$suffix"
        if [[ -e "$candidate" ]]; then
          realpath "$candidate"
          return 0
        fi
      done < <(otool -l "$source" | awk '/cmd LC_RPATH/{getline; getline; print $2}')
      for candidate in "/opt/homebrew/lib/$suffix" "/usr/local/lib/$suffix"; do
        if [[ -e "$candidate" ]]; then
          realpath "$candidate"
          return 0
        fi
      done
      return 1
      ;;
    *)
      return 1
      ;;
  esac
}

is_system_dependency() {
  [[ $1 == /usr/lib/* || $1 == /System/Library/* ]]
}

typeset -A destination_for source_for_basename queued
queue=("$main_source")
destination_for[$main_source]="$staging/tesseract"
source_for_basename[tesseract]="$main_source"
queued[$main_source]=1
index=1
while (( index <= ${#queue[@]} )); do
  source=${queue[$index]}
  (( index += 1 ))
  while IFS= read -r dependency; do
    if is_system_dependency "$dependency"; then
      continue
    fi
    resolved=$(resolve_dependency "$source" "$dependency") || {
      print -u2 "Could not resolve dependency '$dependency' from '$source'"
      exit 1
    }
    if [[ "$resolved" == "$source" ]]; then
      continue
    fi
    basename=${dependency:t}
    if [[ -n ${source_for_basename[$basename]-} && \
          ${source_for_basename[$basename]} != "$resolved" ]]; then
      print -u2 "Dependency basename collision: $basename"
      exit 1
    fi
    source_for_basename[$basename]="$resolved"
    if [[ -z ${queued[$resolved]-} ]]; then
      queue+=("$resolved")
      queued[$resolved]=1
      destination_for[$resolved]="$staging/lib/$basename"
      cp -L "$resolved" "$staging/lib/$basename"
    fi
  done < <(dependencies "$source")
done

for source in "${queue[@]}"; do
  target=${destination_for[$source]}
  while IFS= read -r dependency; do
    if is_system_dependency "$dependency"; then
      continue
    fi
    resolved=$(resolve_dependency "$source" "$dependency") || {
      print -u2 "Could not relocate dependency '$dependency' from '$source'"
      exit 1
    }
    if [[ "$resolved" == "$source" ]]; then
      continue
    fi
    basename=${dependency:t}
    if [[ "$target" == "$staging/tesseract" ]]; then
      replacement="@loader_path/lib/$basename"
    else
      replacement="@loader_path/$basename"
    fi
    install_name_tool -change "$dependency" "$replacement" "$target" 2>/dev/null
  done < <(dependencies "$source")
  if [[ "$target" == *.dylib ]]; then
    install_name_tool -id "@loader_path/${target:t}" "$target" 2>/dev/null
  fi
  codesign --force --sign - "$target" >/dev/null 2>&1
  # Tauri copies resources into target/{profile} during its build script. Keep
  # the staged source owner-writable so a second build can replace the first
  # build's copied resource instead of failing on Homebrew's read-only mode.
  chmod u+w "$target"
  codesign --verify --strict "$target" >/dev/null 2>&1
done

typeset -A recorded_formula
for source in "${queue[@]}"; do
  case "$source" in
    /opt/homebrew/Cellar/*|/usr/local/Cellar/*)
      cellar_tail=${source#*/Cellar/}
      formula=${cellar_tail%%/*}
      version_tail=${cellar_tail#*/}
      version=${version_tail%%/*}
      formula_root=${source%%/Cellar/*}/Cellar/$formula/$version
      if [[ -n ${recorded_formula[$formula]-} ]]; then
        continue
      fi
      recorded_formula[$formula]=1
      license_dir="$staging/licenses/$formula-$version"
      mkdir -p "$license_dir"
      while IFS= read -r notice; do
        cp -L "$notice" "$license_dir/${notice:t}"
      done < <(find "$formula_root" -maxdepth 2 -type f \
        \( -iname 'license*' -o -iname 'copying*' -o -iname 'copyright*' \) | sort)
      for metadata in sbom.spdx.json INSTALL_RECEIPT.json; do
        if [[ -f "$formula_root/$metadata" ]]; then
          cp -L "$formula_root/$metadata" "$license_dir/$metadata"
        fi
      done
      if [[ -z $(find "$license_dir" -type f -print -quit) ]]; then
        print -u2 "No dependency license evidence found for $formula $version"
        exit 1
      fi
      ;;
  esac
done

# Fail closed: a missing or failing scanner must stop staging, never pass.
license_rejected=0
if find "$staging/licenses" -type f -iname '*agpl*' -print -quit | grep -q .; then
  license_rejected=1
fi
sbom_list=$(find "$staging/licenses" -type f -name sbom.spdx.json -print0)
if [[ -n "$sbom_list" ]]; then
  set +e
  printf '%s' "$sbom_list" | xargs -0 grep -Eih \
    '"license(Concluded|Declared)": "[^"]*(AGPL|Affero|LicenseRef-commercial)' >/dev/null
  scan_status=$?
  set -e
  case $scan_status in
    0) license_rejected=1 ;;
    1) ;;
    *) print -u2 "Dependency license scan failed (exit $scan_status)"; exit 1 ;;
  esac
fi
if (( license_rejected )); then
  print -u2 "Rejected dependency license detected in staged OCR resources"
  exit 1
fi
for file in "$staging/tesseract" "$staging"/lib/*.dylib; do
  while IFS= read -r dependency; do
    if [[ "$dependency" != @loader_path/* ]] && ! is_system_dependency "$dependency"; then
      print -u2 "Non-relocatable dependency remains in ${file:t}: $dependency"
      exit 1
    fi
  done < <(dependencies "$file")
done

TESSDATA_PREFIX="$staging/tessdata" "$staging/tesseract" --version >/dev/null
languages=$(TESSDATA_PREFIX="$staging/tessdata" "$staging/tesseract" --list-langs 2>&1)
if ! print -r -- "$languages" | grep -qx 'eng'; then
  print -u2 "Staged Tesseract does not expose the English model"
  exit 1
fi
if ! print -r -- "$languages" | grep -qx 'osd'; then
  print -u2 "Staged Tesseract does not expose the orientation model"
  exit 1
fi
(
  cd "$staging"
  find . -type f ! -name SHA256SUMS -print0 | LC_ALL=C sort -z | xargs -0 shasum -a 256 > SHA256SUMS
)

mkdir -p "$destination"
find "$destination" -mindepth 1 ! -name .gitkeep -exec rm -rf -- {} +
cp -R "$staging"/. "$destination"/
print "Staged relocatable Tesseract at $destination"
du -sh "$destination"
