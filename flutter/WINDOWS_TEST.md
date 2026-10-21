# Windows test build

## Run and download

1. Push the branch yourself (nothing is pushed automatically).
2. GitHub: Actions → "Windows build" → Run workflow → pick the branch.
3. When `flutter build windows --release` finishes, download `docflow-windows-release`.
4. Check the hash: `certutil -hashfile docflow-windows-release.zip SHA256` vs the `.sha256` file.
5. Unzip into an empty folder and run `docflow.exe`.

## Try

- App starts and reaches home without crashing.
- Settings open; language switch works (tr, en, ru, az).
- Fast RFI Scan preview, then Apply and Undo, on a throwaway synthetic folder.

## UNVERIFIED (not run on a real Windows host)

- The build until the workflow succeeds; packaged startup and the checks above.
- Excel COM, Office, Outlook, Microsoft Print to PDF, physical printers.
- Supabase sign-in and RLS; code signing.
