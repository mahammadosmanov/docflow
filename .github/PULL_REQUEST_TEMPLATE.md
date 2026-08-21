## Outcome

<!-- What user-visible result does this PR deliver? -->

## Scope

- Issue:
- Intended version:
- Affected operation/layer:

## Safety impact

- [ ] No filesystem/workbook mutation
- [ ] Preview remains read-only
- [ ] Apply executes only approved steps
- [ ] No-overwrite and journal/undo behavior verified
- [ ] No secrets/private documents included

## Version and documentation

- [ ] `CHANGELOG.md` updated under Unreleased, or not user-visible
- [ ] `releases.json`/`VERSION` changed only for a release PR
- [ ] Product/architecture decision docs updated if the contract changed

## Verification

- [ ] `python tools/version_manager.py check`
- [ ] `.venv/bin/python -m pytest`
- [ ] `.venv/bin/python -m mypy src/docflow`
- [ ] `npm test`
- [ ] `npm run build`
- [ ] `cargo check --manifest-path src-tauri/Cargo.toml`

Paste concise real results and any platform-only omission below.
