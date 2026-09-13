# Changelog

All notable DocFlow changes are recorded here. The project follows Semantic
Versioning; `releases.json` is the machine-readable release catalog.

## Unreleased

Record user-visible changes here while they are being developed. Move them into
a dated version section as part of the release commit.

## 0.18.2 - 2026-09-13

### Fixed

- macOS startup now reads an existing secure session without allowing Keychain
  authentication UI to block the application on its splash screen.
- If a session cannot be read without interaction, DocFlow fails closed to the
  sign-in screen while leaving the Keychain item untouched.

### Safety

- Tokens remain inside the native OS credential store and never cross into the
  webview. Startup does not copy them into settings or weaken credential access.
- Credential creation, update, and deletion behavior is unchanged; only the
  passive startup read is non-interactive.

## 0.18.1 - 2026-09-13

### Fixed

- Device readiness now counts interrupted work through a bounded, read-only
  journal inspection instead of running recovery during dashboard startup.
- A finalized RFI Follow Up recovery notice can now be listed without reopening
  its former workbook root, so a stale path identity no longer hides the
  Recovery Center behind a raw path-grant error.

### Safety

- Startup inspection does not acquire mutation authority, rewrite a journal,
  reopen document roots, or retry an uncertain workbook operation.
- Pending and finalized recovery records remain visible and are never silently
  discarded or treated as safe to apply again.

## 0.18.0 - 2026-09-13

### Added

- Guarded RFI System and Follow Up updates now expose an expandable exact cell
  comparison before Apply: workbook, sheet/cell, RFI, current value, and new value.

### Safety

- The comparison reads only expected and replacement values already embedded in
  approved workbook steps. It does not reopen Excel or recalculate mutations.
- Invalid coordinates, oversized text, unknown workbook kinds, and non-canonical
  RFI links fail closed; display is capped at 1,000 approved cell changes.

### Fixed

- Corrected the release-safety document's stale operation count from eight to nine.

## 0.17.0 - 2026-09-13

### Added

- Warning-heavy previews now provide compact text search, warning-type filtering,
  visible result counts, and a bounded scrolling list.
- Exact Worley RFI warnings can open their related old/new first-page comparison
  without making the operator search the page selector manually.

### Safety

- Filtering is presentation-only: it never removes warnings from the approved
  Plan or changes Apply eligibility.
- Page focus requires an exact ASCII canonical RFI code already present in the
  signed-page editor. Corrupted or decorated identifiers cannot create a link,
  and focus never selects an archive or revises executable steps.

## 0.16.0 - 2026-09-13

### Added

- Worley signed-page staging now shows the current archive first page beside the
  exact new signed page selected by the approved Plan.
- Ambiguous archive choices update the old-page image together with the selected
  archive candidate.

### Safety

- Rust generates read-only grayscale PNG thumbnails only for the pinned Plan
  pages; source scans and archives are never changed during preview.
- Preview is capped at 20 RFI rows, 256 KiB per PNG, 4 MiB total encoded data,
  and 240 by 320 pixels. Missing or unsupported previews require manual review
  and never affect Apply authority.
- React accepts only bounded PNG-signature data values and receives no local file
  URL or added filesystem authority.

## 0.15.0 - 2026-09-13

### Added

- Added a collapsed exact-effects panel to schema-driven editable previews.
- Operators can expand it before Apply to inspect every approved non-directory
  effect with its complete source and destination paths.

### Safety

- The panel reads only immutable Plan steps already approved by Rust; opening it
  performs no filesystem read, recalculation, or mutation.
- The list is height-bounded and scrollable so large plans do not hide the
  editor or Apply controls.

## 0.14.0 - 2026-09-13

### Added

- Added a compact Apply-impact summary to every ordinary and editable preview.
- The summary separately counts new outputs, updated files, moves or renames,
  print jobs, Outlook drafts, cleanup actions, and warnings.

### Safety

- Impact counts are derived only from the immutable Rust-approved Plan and do
  not create, remove, or authorize effects.
- The summary explicitly reminds the operator that preview has not changed any
  document and Apply can execute only the approved preview.

## 0.13.1 - 2026-09-13

### Fixed

- Combined Worley scan preview now reports the page currently being analyzed
  and responds to cancellation between pages instead of waiting for the entire
  PDF to finish.

### Safety

- Cancellation during page recognition remains preview-only and creates no
  review output, journal, source split, or archive change.

## 0.13.0 - 2026-09-13

### Added

- Added page-by-page RFI recognition for a single combined Worley scan, so each
  confidently identified page becomes its own review row and exact staging step.
- Added page numbers to signed-scan review labels and four-language guidance for
  combined scans.

### Safety

- Preview pins the whole signed PDF fingerprint and exact page index; apply does
  not split, modify, or re-read the source to rediscover work.
- Unreadable, low-confidence, duplicate, unsupported, missing, and ambiguous
  pages produce no output. The archive remains read-only and existing review
  files remain protected from overwrite.

## 0.12.0 - 2026-09-13

### Added

- Added owner-scoped RFI work audit export as a lossless JSON file and an
  Excel-safe CSV file from one previewed operation.
- Added live event-count, storage-size, and 10,000-event capacity visibility to
  the RFI work dashboard.

### Safety

- Audit files use unique names, never overwrite an existing destination, and
  remain journaled and undoable. Invalid or foreign-owner events are excluded.
- Formula-like CSV text is neutralized before Excel opens it, each output is
  bounded to 16 MiB, and export never deletes or changes work-item history.

## 0.11.0 - 2026-09-13

### Added

- Added one-click launch of the next journal-producing operation from eligible
  RFI work cards: RFI Log after operator checking, Process Scans after the
  publication email is archived, and System Sync after the INTEC scan is filed.
- Added a separate preview-first work-item update after a fully successful
  operation, linked to the exact local journal that proved the effect.
- RFI Log evidence can populate newly assigned exact RFI numbers; scan and
  Worley-sync evidence must cover every RFI already attached to the work item.

### Safety

- Partial, cancelled, failed, missing, foreign-owner, undone, mismatched, and
  unsupported journals cannot advance an RFI work stage.
- Process Scans counts only successfully filed document steps, not `_review`
  routing, and Worley synchronization requires the same RFI to be proven in
  both committed workbook-update steps.
- Outlook drafts never advance publication or archive stages. If previously
  accepted source evidence later becomes invalid, the work item moves to Needs
  Review instead of retaining an unsupported success state.

## 0.10.1 - 2026-09-13

### Fixed

- Localized the shared back-button accessible label and the home-screen
  preview-first safety badge in Turkish, English, Russian, and Azerbaijani.

## 0.10.0 - 2026-09-13

### Added

- Made every Today, Resume, and Needs Review card directly open its existing
  RFI work item in the shared schema-driven editor with the item identity,
  current stage, due date, and review state already filled.

### Safety

- Added a reusable schema-level `remember` policy and marked RFI work-item
  fields transient, so task identities and prior transition values cannot leak
  into the next new item through saved form preferences.
- The shortcut still produces the same preview-first, explicitly approved,
  no-clobber, journaled work-item event; it adds no document mutation path.

## 0.9.0 - 2026-09-13

### Added

- Added offline-first RFI work items with explicit lifecycle stages, optional
  due dates, strict canonical RFI numbers, review flags, and mandatory reasons
  for manual stage changes.
- Added compact Today, Resume, and Needs Review queues to the home screen.
- Added an authenticated, bounded work-item query and a schema-driven operation
  for creating or advancing work without an operation-specific React form.

### Safety

- Every work-item event is previewed, explicitly approved, written as a new
  no-clobber file, journaled, and undoable; official Excel files and RFI folders
  remain unchanged and authoritative.
- Event storage is append-only, owner-scoped, size/count bounded, and rejects
  symlinks, malformed records, non-ASCII RFI codes, and unknown stages.
- Concurrent transitions never use last-writer-wins; a stale branch sends the
  item to Needs Review while retaining the last proven stage.

## 0.8.0 - 2026-09-13

### Added

- Promoted the device-readiness foundation to a SemVer minor release because it
  adds a backward-compatible user capability.
- Added a tested `next fix|feature|breaking` version command so completed work
  is classified consistently instead of extending one prerelease number.

### Changed

- Established PATCH for compatible fixes, MINOR for compatible features, and
  MAJOR for incompatible contract changes as the mandatory commit workflow.

### Safety

- Version correction was committed separately without rewriting earlier Git
  history, preserving exact rollback points for the preceding implementation.

## 0.7.0-rc.11 - 2026-09-13

### Added

- Added an authenticated, read-only **Device readiness** panel to the home
  screen. It checks bundled OCR/language availability, enumerated printers, and
  interrupted journal or print-queue recovery work without scanning user files.
- Added one-click navigation from an attention result to Recovery Center and a
  manual refresh action.
- Added the accepted offline-first RFI work-item lifecycle and evidence boundary
  as ADR 0002, ready for incremental implementation.

### Changed

- Reconciled the README and roadmap with the current seven-operation Rust
  application and the 0.7 reliability/unified-work priorities.

### Safety

- A healthy device-readiness result is intentionally not treated as Excel,
  OneDrive, Office, or printer-output certification. File-specific safety checks
  remain in operation preview and platform certification remains separate.

## 0.7.0-rc.10 - 2026-09-13

### Fixed

- Corrected **Analyze RFI System & Sync Worley Signatures** against the current
  production workbook layouts:
  - discovers each RFI Log `RFI No.` and `REMARKS` column from exact headers
    instead of writing to former fixed columns 20/21, which are current
    shared-formula Folder Link columns;
  - updates only literal `REMARKS` cells and the `Signed By Worley CIV`
    `STATUS` / `Signed By Worley MEC` `Worley Status` control cells;
  - preserves existing remarks by appending the exact `Signed by Worley` line;
  - rejects missing, merged, formula-bearing, unexpected, stale, or altered
    targets before unsafe mutation;
  - applies a closed package-delta verifier so shared formulas, `calcChain.xml`,
    VBA, styles, links, and every non-target OOXML member remain unchanged;
  - revalidates both workbook fingerprints before the first mutation and again
    before each workbook step;
  - supports actual per-RFI selection during plan revision.

### Verification

- Added an operator-supplied production-template acceptance test. Copies of the
  current RFI Log and Inspection Status Follow Up passed guarded preview/output
  verification without changing their source files.

## 0.7.0-rc.9 - 2026-09-12

### Fixed

- Resolved `path grant does not allow this access` (`BackendError::path_access_denied`) in Analyze RFI System & Sync Worley Signatures (`rfi.system_sync`):
  - Fixed path grant selection in `preview_rfi_system_sync` which previously defaulted to matching the read-only `system_root` grant when RFI Log or Follow Up workbooks were stored inside the selected system folder.
  - Dynamically promoted `system_root` provisional grant to `ReadWrite` when workbooks are nested inside or equal to the system directory, avoiding conflicting capability grants and safely granting write authority.
  - Explicitly resolved grants with write intent (`g.access.allows(PathIntent::Write)`), and resolved candidate paths against canonical capabilities in `authorize_capability_for_grant`.
  - Added missing `provisional.commit()` to preserve granted path capabilities across preview -> apply boundaries.
  - Fixed `reserved plan ID requires an empty plan ID` by initializing `Plan` with an empty `plan_id` for reserved plan binding.

## 0.7.0-rc.8 - 2026-09-12

### Added

- Added **Analyze RFI System & Sync Worley Signatures** (`rfi.system_sync`) operation:
  - Scans single RFI system folder structure containing discipline directories (`CIV`, `MEC`, `ELE`, `PRC`) and canonical RFI folders (e.g. `AZE-SUN-INT-QMS-RFI-CIV-0103`).
  - Inspects scanned PDF files matching `RFI No. ...` for Worley's signature in the page 1 bottom-right signature box labeled `Others` (identifying signatures such as E.Mammadov via dark ink density, handwriting, and stamp recognition).
  - Synchronizes RFI Log discipline sheets (`CIV`, `MEC`, `ELE`, `PRC`): writes `Signed by Worley` in the `REMARKS` column (column 20 for CIV/ELE/PRC, column 21 for MEC) for signed RFIs, leaving unsigned RFIs untouched.
  - Synchronizes Follow Up Final ITP sheets (`Signed by CIV` and `Signed by MEC`):
    - Civil: maps ITP item `2.3.1` to Follow Up column `3.1`, matched by `Area` column == RFI Log `Tag No.`, writing short RFI code (e.g. `CIV-0103`).
    - Mechanical: maps ITP items `1.3.5` -> column `3.5`, `3.3.1` -> column `3.1`, and `4.3.2` -> column `3.2`, matched by `PCS number` == `tagNo`, `Section` == `area`, and `Area` == `location`, writing short RFI code (e.g. `MEC-0700`).
  - Generates comprehensive monochrome preview table with discipline filtering, status details, and individual toggle checkboxes.
  - Multi-workbook apply with automatic no-clobber backups and lossless journaled undo.
  - Complete localized UI strings in `en`, `tr`, `ru`, and `az`.

### Fixed

- Resolved `More than one safe Follow Up table was found. Keep one exact RFI NO./STATUS table.` (`error.rfi.followUp.layoutAmbiguous`) in Worley signature status synchronization when handling real-world Follow Up workbooks containing multiple discipline sheets (e.g. `Signed by CIV` and `Signed by MEC`).
- Added multi-sheet layout discovery and atomic preview/apply synchronization across all valid discipline Follow Up sheets in a single transaction.
- Added smart sheet disambiguation prioritizing recognized discipline conventions (`SIGNED BY`, `FOLLOW UP`, `CIV`, `MEC`, `ELE`, `PRC`) while preserving strict ambiguity rejection for unknown dummy sheets.

## 0.7.0-rc.6 - 2026-09-12

### Fixed

- Resolved `unknown workbook relationship class` validation error in Worley signature synchronization (`rfi.sync_worley`) when loading real-world Excel workbooks (`.xlsm` / `.xlsx`).
- Added full support for Microsoft Office relationship namespaces across versions (e.g. `2006`, `2007`, `2011`, `2014`, `2017`, `2019`, `2020`) and package-level relationship namespaces.
- Expanded allowed workbook and worksheet relationship classes to recognize standard Excel elements including `volatileDependencies`, `calcFeatures`, `features`, `vbaProjectSignature`, `customXmlMappings`, `xmlMaps`, `customData`, `customDataProperties`, `customUI`, `attachedToolbars`, `activeXControlBinary`, `usernames`, `revisionLog`, and `revisionHeaders`.

## 0.7.0-rc.5 - 2026-09-11

### Fixed

- Eliminated UI button overlap and obscured form fields by decoupling action bar from scrollable form/review content (`flow-scroll`) and pinning action buttons cleanly at the bottom.
- Resolved unclosed `@media (prefers-color-scheme: dark)` brace in `styles.css` that leaked media query scope across subsequent global styles.
- Fixed detached table header bug where regular data columns stuck to `right: 0` during horizontal scrolling when row actions were disabled; now explicitly targeted with `editor-actions-header`.
- Added clear separation border and shadow to sticky row action columns to prevent scrolled table cells from bleeding behind buttons.
- Added `white-space: nowrap` and `flex-shrink: 0` to `.button` and icon buttons to prevent label wrapping and squishing across different window sizes and localized languages.
- Added auto-close behavior and focus management to `CompactDateInput` year select to prevent overlaying rows beneath it.
- Added 1-click clear button (`X`) to path inputs when a file or folder is selected.
- Added `max-height` and scrolling to `.modal` to guarantee action buttons are always accessible on low-height viewports.

## 0.7.0-rc.4 - 2026-09-11

### Added

- High-contrast monochrome dark mode adhering to Rule 11 with obsidian backdrop (`#0a0a0a`), clean surface tiers (`#141414`, `#1e1e1e`, `#2a2a2a`), crisp white text, and distinct card borders.
- 1-click header theme switcher toggling between Light, Dark, and System with `Alt + T` operator keyboard shortcut.
- Full localization in Turkish, English, Russian, and Azerbaijani.

## 0.7.0-rc.3 - 2026-09-11

### Added

- Live preview table search box and instant filter tabs (`All`, `Changes`, `Manual Review`) with dynamic filtered row count.
- Global operator keyboard shortcuts: `Ctrl/Cmd + Enter` to trigger Preview or Apply, `Escape` to return to form or close modals, and `Space` to toggle row selection or enable/disable state.
- Localized filter labels in Turkish, English, Russian, and Azerbaijani.

## 0.7.0-rc.2 - 2026-09-11

### Added

- Automated cross-compilation pipeline for Windows x64 (`tools/build_windows_cross.sh`) packaging standalone Tesseract OCR binaries and `eng.traineddata` into both NSIS installer and zero-install portable zip.
- HTML5 drag-and-drop on all single and multi-file/folder form inputs with monochrome active drop states.
- Automatic parameter persistence and re-issue of opaque path tokens on operation load.
- 1-click diagnostic report download in Preferences -> Support.
- Robust header discovery across non-standard contractor RFI logs (detecting RFI and Worley status columns across first 10 rows and 64 columns).

### Safety

- Preserved strict non-worksheet sheet relationship checks in OOXML workbook discovery.
- All 321 Rust tests and 39 frontend/tooling tests passing with zero warnings.

## 0.7.0-rc.1 - 2026-08-29

### Added

- Exact, package-preserving RFI Follow Up preview, revision, apply, recovery,
  and targeted undo with bounded authority and no-clobber publication.
- Windows-only Outlook Reply All draft preparation that revalidates the exact
  selected message, preserves the existing quote/signature, displays only an
  unsent draft, and never synthesizes recipients or calls Send.
- A bounded, redacted, local-only diagnostics preview/copy surface with no
  telemetry or user-document scan.
- Commit-bound Windows installer, checksum, smoke, cold-start, peak-memory,
  package-audit, uninstall, and artifact-manifest evidence.

### Safety

- RFI matching is exact whole-value matching; only `Completed → Completed` and
  `Ongoing → In Progress` are automatically selectable.
- Outlook draft undo is available only for the exact unchanged durable draft;
  otherwise it fails closed.

### Changed

- The desktop application authority is fully Rust-owned; the retired application
  sidecar, interpreter runtime, child-process routing, and frozen packaging have
  been removed.
- Version, source/package audit, dependency-license, checksum, and packaged-smoke
  gates now run with Node, Rust, and platform-native tools only.

### Safety

- Final builds fail fast if retired runtime paths or shell/external-binary wiring
  reappear, and package inspection rejects interpreter libraries, bytecode,
  frozen-runtime artifacts, or an application sidecar.
- The packaged binary smoke verifies isolated bootstrap/settings plus real OCR and
  Process Scans apply/journal/undo before an artifact can be accepted.

## 0.6.0 - 2026-08-21

### Added

- Tauri 2 + React/TypeScript desktop shell while preserving the proven Python
  document engine and private JSON-lines sidecar boundary
- Safe signed-RFI first-page staging with read-only archive guarantees,
  duplicate/ambiguous match reporting, and original folder-structure retention
- Persistent, cancellable, resumable mixed-file print queues with per-file,
  group, and all-files configuration
- Compact individual, sheet, RFI-group, and all-row RFI Log editing
- Canonical `VERSION`, machine-readable `releases.json`, repository-wide version
  checks, complete historical changelog, and tag-driven release workflow

### Changed

- The active UI stack moves from Flutter/PySide rollback implementations to a
  lean Tauri webview; legacy settings, journals, trash, and Supabase rows remain
  compatible
- Windows packages are native NSIS installers with the Python sidecar and pinned
  Tesseract bundled without a companion console window

### Safety

- Webview code never receives Supabase access/refresh tokens or arbitrary
  filesystem/network authority
- Every mutation remains preview → approval → apply → journal → undo
- Signed archive PDFs remain read-only; publication never overwrites an existing
  review file and recovers safely from OneDrive/antivirus locks
- Release builds require synchronized Python, Node, Rust, Tauri, and lockfile
  versions before packaging

## 0.5.4 - 2026-08-03

### Fixed

- RFI inspection description metinlerindeki UTF-8 bullet karakterinin Windows
  code-page katmaninda `â€¢` olarak gorunmesi; sidecar JSON hatti artik Unicode'u
  ASCII-safe escape'lerle tasiyor ve istemci dogru `•` karakterini geri kuruyor
- `CIV-5.2.2` ve `CIV-5.2.3` icin description, forms, AZFEN type ve INTEC type
  degerleri uretim workbook'undaki gercek Sheet1 kayitlariyla sabitlendi

### Safety

- Workbook'tan okunan tum lookup metinleri NFC normalize edilir ve yalnizca
  guvenli oldugunda UTF-8/CP1252 mojibake otomatik onarilir
- Tum CIV, MEC, ELE ve PRC presetlerinde bos alan, uyusmayan AZFEN/INTEC item
  baglantisi veya cakisan duplicate tespit edilirse preview artik durur
- 234 uretim presetinin tamami gercek RFI Log uzerinde read-only denetlendi;
  eksik, cakisan veya supheli encoding kaydi bulunmadi

## 0.5.3 - 2026-08-01

### Added

- MEC RFI kayitlarinda `ROW No.` alani artik karma sheet editorunde gorunur ve
  duzenlenebilir; deger Excel'deki gercek H sutununa yazilir ve dogrulanir
- MEC icin Excel tabanli mail onizleme/kopyalama `ROW No.` basligini, degerini ve
  kaynak workbook bicimini sheet'e ozel 15 kolonlu secime dahil eder

### Changed

- Item No. secenekleri UI'da `CIV-`, `MEC-`, `ELE-` ve `PRC-` onekleri olmadan
  gosterilir; plan ve Excel'e yazilan asil deger tam disiplin onekini korur
- Kompakt RFI satir aksiyonlari Material'in guvenli tiklama alanini koruyacak
  genislikte tutulur; uc buton artik hucre disina tasmaz

### Safety

- Karma CIV/MEC editorunde yalniz MEC Row No. hucresinin duzenlenebilir oldugunu,
  MEC H sutununa yazmayi ve workbook-backed kopyaya alinmasini kalici testler
  dogrular

Bu proje [Semantic Versioning](https://semver.org/) kullanır. Kullanıcıya görünen
her feature veya fix, yeni Windows paketi üretilmeden önce burada kaydedilir.

## 0.5.2 - 2026-07-31

### Fixed

- Sheet bazlı Excel kopyalama butonlarının dar pencerelerde Done butonunun
  arkasına taşması; kopyalama aksiyonları artık kendi tam genişlikte, satır
  kırabilen alanında ve alt footer'ın üzerinde duruyor
- Çoklu-sheet RFI işleminde inspection saatlerinin bütün sheet'ler için tek
  ortak zaman çizelgesinden verilmesi; CIV, MEC, ELE ve PRC artık yalnızca kendi
  sheet'indeki mevcut saatleri devam ettiriyor

### Safety

- Dört kopyalama butonunun kompakt pencere genişliğinde Done alanıyla
  çakışmadığını doğrulayan widget testi
- CIV overflow, MEC devam saati ve bağımsız ELE/PRC başlangıç saatlerini aynı
  çoklu-sheet planında doğrulayan kalıcı regresyon testi

## 0.5.1 - 2026-07-30

### Fixed

- Yerel Windows paketinde PyInstaller sidecar klasörünün yanlışlıkla bir seviye
  fazla içeri kopyalanması nedeniyle arka plan servisinin başlayamaması
- Uygulamanın standart düz paket yerleşimine ek olarak iç içe PyInstaller
  yerleşimini de güvenli fallback olarak algılaması

### Safety

- Yerel runner tabanlı paketleyici sidecar klasörünün kendisini değil içeriğini
  kopyalıyor, beklenen `sidecar\docflow_sidecar.exe` yolunu zorunlu tutuyor ve
  paketi teslim etmeden önce gerçek executable ile `ping` çalıştırıyor
- Düz ve iç içe paket yerleşimleri ile geliştirme override'ı Flutter testleriyle
  kalıcı olarak doğrulanıyor

## 0.5.0 - 2026-07-30

### Added

- CIV, MEC, ELE ve PRC için aynı işlem içinde ayrı adetler seçebilen kompakt
  çoklu-sheet RFI seçim alanı
- Sheet bilgisi görünür tek düzenleme tablosunda tüm otomatik alanları değiştirme,
  Area bazlı aynı RFI satırını çoğaltma ve özel değer yazma desteği
- İşlem sonucunda yalnızca kayıt eklenen sheet'ler için ayrı Excel-native
  kopyalama butonları

### Changed

- RFI numarası, sonraki satır ve ITP/Item eşleşmeleri her sheet için bağımsız
  hesaplanırken inspection saatleri tüm sheet'lerde çakışmayı engelleyecek şekilde
  ortak planlanıyor
- Mail okuma CIV/MEC ile sınırlı değil; ELE ve PRC disiplinlerini de algılıyor
- Çoklu-sheet apply/undo işlemi tek workbook yazımı ve tek geri alma kaydıyla
  atomik olarak çalışıyor

### Safety

- CIV, MEC, ELE ve PRC formülleri, farklı sütun yerleşimleri, item/duplicate veri
  doğrulamaları ve dokunulmayan Expiry/Status/Validity alanları kalıcı testlerle
  korunuyor
- 18 binden fazla satırlı gerçek RFI Log güvenli kopyasında dört sheet birlikte
  uygulandı, sheet bazlı mail tabloları tekrar Excel'den okundu ve undo'nun dosyayı
  hash seviyesinde birebir geri getirdiği doğrulandı

## 0.4.4 - 2026-07-30

### Fixed

- Excel penceresinin filtreleme ve native clipboard kopyası tamamlanmadan
  görünmesi; kullanıcının seçim veya hücre değişikliğiyle COM otomasyonunu
  durdurabilmesi
- Excel artık hazırlık boyunca gizli, ekran güncellemesi ve kullanıcı
  etkileşimi kapalı çalışıyor; yalnız kopya hazırlandıktan sonra görünür ve
  etkileşimli hale geliyor

### Safety

- Script sırası için kalıcı test: Excel görünürlüğü ve etkileşimi yalnız
  `Range.Copy()` tamamlandıktan sonra açılabilir
- Gerçek Excel integration testi native Biff12/HTML/RTF biçimlerini ve doğru
  iki RFI satırını gizli hazırlık akışıyla doğruluyor

## 0.4.3 - 2026-07-30

### Fixed

- Birden fazla farklı RFI numarası kopyalanırken bazı Office type-library
  sürümlerinin `AutoFilter(..., 7)` COM çağrısını reddetmesi
- Çoklu seçimde aynı native Excel görünümünü problemli COM overload'u yerine
  hedef satırların görünürlüğünü doğrudan yöneterek oluşturma
- Başarısız Excel otomasyonunun boş `EXCEL.EXE /automation -Embedding`
  süreçleri bırakması; hata sinyali cleanup sonrasına taşındı ve PID tabanlı
  ikinci kapatma güvencesi eklendi

### Safety

- Gerçek Microsoft Excel integration testi iki farklı RFI için genişletildi:
  yalnız hedef satırlar, header ve Subcontractor kopyalanırken eski RFI ve
  Expiry panoya alınmıyor
- Kontrollü hata testinde yeni Excel süreci kalmadığı doğrulandı

## 0.4.2 - 2026-07-30

### Fixed

- 18 binden fazla satırlık RFI Log dosyasını kopyalama öncesinde rastgele hücre
  erişimiyle tekrar tekrar tarayan ve butonu süresiz yükleniyor durumunda
  bırakan okuma yolu tek geçişli akışa çevrildi
- Gerçek boyuttaki workbook'ta mail tablosu verisi saniyeler içinde hazırlanıp
  Excel'in native biçimli panosuna aktarılıyor

### Safety

- Büyük read-only worksheet'lerde veri satırlarına rastgele erişimi reddeden
  kalıcı regresyon testi eklendi
- Üretim boyutundaki güvenli workbook kopyasıyla Excel Biff12/Biff8, HTML ve
  RTF pano formatları uçtan uca doğrulandı

## 0.4.1 - 2026-07-30

### Fixed

- Güncel RFI Log dosyasında K:Q'dan L:R'ye kayan Sheet1 eşleme kolonlarının
  header yapısından dinamik bulunması
- Yeni CIV/MEC satırlarında VLOOKUP formüllerinin gerçek eşleme kolonlarıyla
  yazılması; calculated-column ve data-validation uyumluluğunun korunması
- Excel'deki bozuk ayraçlı ITP adlarının normal mail metniyle eşleşmesi ve
  birleşik MEC Tag değerlerinin kısaltılmadan çıkarılması
- Uygulama içindeki yaklaşık mail tablosu önizlemesi yerine gerçek workbook'un
  Excel'de filtrelenip header dâhil native Office formatlarıyla kopyalanması

### Safety

- RFI yazma testi gerçek üretim workbook kopyasında Excel COM ile açılma,
  formül tutarlılığı ve data-validation sonuçları üzerinden doğrulandı
- Kopyalama yalnız Excel'in kendi panosunu kullanır; Biff, HTML, RTF ve Office
  bağlantı formatları uygulama tarafından yeniden yazılmaz

## 0.4.0 - 2026-07-29

### Added

- Process Scans için varsayılan mevcut klasörlü davranışı koruyan, isteğe bağlı
  klasörsüz çıktı modu
- Klasörsüz modda adlandırılmış PDF'leri doğrudan seçilen hedef klasöre
  taşıma veya orijinalleri koruma seçiliyse kopyalama

### Safety

- Eski kayıtlı ayarlar ve eski istemci istekleri için klasörlü davranışın
  otomatik olarak korunması
- Process Scans klasörlü/klasörsüz hedef, kopyalama, apply ve diğer özellikler
  için genişletilmiş regresyon kontrolleri

## 0.3.0 - 2026-07-29

### Added

- Uygulama sonucunda Excel'den yeniden okunan başlık ve hücrelerle Outlook
  mail tablosu önizleme penceresi
- Excel kolon genişliklerini kullanan yatay/dikey kaydırılabilir, hücre metni
  seçilebilir tablo ve tek tıklamalı biçimli kopyalama

### Fixed

- Excel'in clipboard HTML belgesindeki `<head>` CSS stillerinin korunması
- Unicode içerikte CF_HTML byte offset'lerinin yeniden hesaplanarak Outlook'a
  eksik veya bozuk tablo yapıştırılmasının önlenmesi

## 0.2.4 - 2026-07-28

### Fixed

- Gemini REST `TextResponseFormat.MimeType` alanında metin MIME değeri yerine
  zorunlu `APPLICATION_JSON` enum değerinin gönderilmesi
- Güncel `responseFormat`, modern `responseJsonSchema` ve legacy
  `responseSchema` arasında üç kademeli structured-output uyumluluğu

## 0.2.3 - 2026-07-28

### Fixed

- Gemini Generate Content API'nin güncel `responseFormat` structured-output
  biçimine geçiş
- HTTP 400 uyumsuzluğunda güncel ve legacy istek biçimleri ile desteklenen
  modeller arasında kontrollü otomatik fallback
- `N3_PCS-02_S7/S8` gibi escaped underscore ve slash içeren MEC Tag
  numaralarının bozulmadan çıkarılması
- Kısa MEC Item numaralarının seçilen ITP eşleşmesiyle offline çözülmesi

## 0.2.2 - 2026-07-27

### Fixed

- Yeni kullanıcılara kapatılan `gemini-2.5-flash-lite` varsayılanı yerine
  ücretsiz katmanı bulunan `gemini-3.1-flash-lite` kullanılması
- Seçilen Gemini modeli 404 döndürürse güncel Flash modellerine otomatik geçiş
- Yeni Gemini modellerinde deprecated olan `temperature` parametresinin kaldırılması

## 0.2.1 - 2026-07-27

### Fixed

- Gemini structured-output şemasındaki geçersiz boş discipline enum değeri
- Markdown biçimli Azerice maillerde RFI adedi çıkarımı
- `PCU-14/15/16` gibi kompakt Tag listelerinin ayrı satırlara genişletilmesi
- `2.1` gibi kısa Item numaralarının seçilen ITP eşlemesiyle çözülmesi

## 0.2.0 - 2026-07-27

### Added

- RFI talep e-postalarından offline veya isteğe bağlı Gemini Free ile taslak çıkarma
- Excel'in yerel biçimini koruyan kesin RFI satırı kopyalama
- Uygulamanın sol altında kalıcı sürüm rozeti
- App, sidecar ve build manifesti arasında zorunlu sürüm eşleşme kontrolü
- Küçük ve hızlı Windows UI güncelleme artifact'i

### Fixed

- RFI Log data validation ve calculated-column uyumluluğu
- Process Scans OCR regresyon koruması
- Çoklu dosya yazdırmada kesin sıra ve kullanıcı müdahalesi bekleme

## 0.1.0 - 2026-07-26

- İlk doğrulanmış Windows paketi
- Process Scans, RFI Log ve Batch Print işlemleri
