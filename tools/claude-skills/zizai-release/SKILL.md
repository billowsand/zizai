---
name: zizai-release
description: >
  Run a 字在 Zizai Windows release end-to-end: pick the next version, edit
  the five apps/windows/{server,tsf,settings,settings-egui,voice-worker}
  Cargo.toml files plus Cargo.lock, update CHANGELOG.md, commit, push to main,
  tag windows-v<version>, and watch release.yml until the GitHub Release is
  published. Also covers the bundled variant (windows-v<version>-bundled)
  that uploads an extra Zizai-<version>-bundled-Setup.exe with embedded
  SenseVoice / punctuation / homophone-replacer models and the local
  sentence-level model to the same GitHub Release.
  Use when the user says "发布", "release", "发版", "windows-v<version>",
  "bundled", "0.1.x", or asks to add the bundled variant of an already
  released version.
---

# Zizai Release Skill

A complete, opinionated runbook for cutting a 字在 Zizai Windows release,
including the bundling of off-chain hosted AI models into the same Release.
The discipline in `docs/notes/release.md` is enforced by a sequence of
git/CI commands; this skill packages the sequence so a fresh agent can
execute it without rediscovering the pitfalls.

## When to invoke

- User wants to ship a new version of Windows product: any phrasing with
  "release", "发版", "发布", "windows-vX.Y.Z", "0.1.X", "cut a release".
- User wants the **bundled** variant of an existing version: "带模型",
  "打包一份带所有模型的版本", "离线也能用的安装包", "bundled-Setup.exe".
- User wants a re-run because the previous release workflow failed:
  "rerun release", "rebuild 0.1.10", "上一次 release 跑挂了".

Do NOT invoke this skill for:

- Linux / macOS releases — `macos-v*` is a different workflow owned by the
  upstream qingjian-team repository.
- Docs-only changes — those go through `ci.yml` on a normal push, not a tag.
- Building installers for local debugging — use `apps/windows/installer/build.ps1`
  directly, no tag.

## Inputs

Ask the user — and DO NOT GUESS — for any of the following when unclear:

| Decision | Default if user does not answer |
|---|---|
| Next version number | ask; never invent |
| Channel (`alpha` / `beta` / `rc` / `stable`) | `beta` |
| Bundled variant requested? | no — they have to ask |
| Push to which remote | `origin` (the upstream `qingjian-team/qingjian` is configured but not used for releases in this fork; check `git remote -v` and confirm) |
| SignPath signing enabled? | read `gh secret list -R <owner>/<repo>`; if both `SIGNPATH_API_TOKEN` and `SIGNPATH_PROJECT_SLUG` are present, signing is on, otherwise skip |
| Allow model-merged single-pass for CHANGELOG draft? | yes (per `docs/notes/release.md` 第 2 步 — system prompt in `docs/notes/release.md` § Appendix) |

## What this skill produces

1. **Conventional release** (`windows-v<version>`): one GitHub Release
   titled "字在 Windows <version>" containing
   - `Zizai-<version>-Setup.exe`
   - `SHA256SUMS`
   - `build-info.json`
   - `releases.json` (regenerated, with the new release on top)

2. **Bundled variant** (`windows-v<version>-bundled`): same Release, but
   additionally
   - `Zizai-<version>-bundled-Setup.exe` (~295 MB, contains SenseVoice
     int8 + punctuation CT-Transformer + homophone-replacer lexicon/FST
     + `data/model/model.qjm`).
   - `releases.json` regenerated against the base release tag.

## Operating procedure

### 0. Pre-flight (always)

```bash
cd D:/qingjian                                    # or wherever the repo lives
git status                                        # must be clean
git fetch origin main
git log -1 --format='%H %s' origin/main
git remote -v                                     # verify the push target
gh auth status                                    # verify gh CLI works
ls tools/release/                                 # data-bundle.sh, pack-model.sh, ...
```

If the working tree is dirty or behind main, STOP and ask the user to clean
up. Tagging a dirty HEAD is the most common source of "release.yml failed
at Check version".

### 1. Conventional release — pick a version

Decide the next version by:

1. List the existing tags:
   ```bash
   git tag -l 'windows-v*' --sort=-v:refname
   ```
2. Read the most recent CHANGELOG.md heading.
3. Apply the project's rules from `docs/notes/release.md`:
   - patch bump for fixes only
   - minor bump for new user-facing features
   - `alpha`/`beta`/`rc` suffix in pre-release channels; the file names
     `0.1.x` for released builds, `0.1.x-dev-<hash>` for in-progress builds

### 2. Conventional release — prepare the changes

Four files change in this exact order, on a single feature branch off main:

| File | Change |
|---|---|
| `apps/windows/{server,tsf,settings,settings-egui,voice-worker}/Cargo.toml` | set `version = "<version>"` (no `-dev`) in all five. The `-dev` suffix is for in-progress development; release commits remove it. |
| `Cargo.lock` | run `cargo update --workspace` and commit the resulting lock change. The lock holds the five windows crate versions, so they will move from `-dev` to the new version. |
| `CHANGELOG.md` | prepend a new `## <version> · <YYYY-MM-DD> · <channel>` section. Per `docs/notes/release.md` § Appendix, drafts CAN come from an LLM using the embedded system prompt, but every line must be reviewed by a human before commit. Internal jargon (StepDown, Sentence kind, Sentence, embed-manifest, uiAccess, build.rs) is forbidden. |
| (optional) `docs/notes/release.md` | if the release process changed (e.g. CI fix), record it. |

Commit message format (Conventional Commits):

```
chore(release): <version>

<2-4 line body explaining what changed in this version, written for users
not engineers>
```

Push to main, wait for ci.yml to be green on both `core` and `windows` jobs
(typical ~3 min on Windows, ~50 s on Linux). Use `gh run watch` to follow.

### 3. Conventional release — tag

```bash
git tag -a windows-v<version> -m "字在 Zizai Windows <version>"
git push origin windows-v<version>
```

The CI runs `release.yml`. Watch it with:

```bash
gh run list -R <owner>/<repo> -L 3 --json databaseId,name,headSha \
  --jq '.[] | select(.name=="release") | {id: .databaseId, status, sha: .headSha[0:7]}'
gh run watch <databaseId> -R <owner>/<repo> --exit-status
```

The job has three "choke" steps that historically failed:

- `Check version matches tag and tag is on main`: if it fails, the tag's
  commit is not on `origin/main`, or the version in
  `apps/windows/server/Cargo.toml` is still `-dev` or doesn't match.
- `Pre-fetch sherpa-onnx native lib`: this is the step that broke in
  0.1.11 — `sherpa-onnx-sys`'s build.rs occasionally can't find the
  downloaded `sherpa-onnx-c-api.lib` on a fresh runner. The current
  release.yml pins the archive SHA-256 and uses PowerShell `tar` + sets
  `SHERPA_ONNX_LIB_DIR`. If it fails again, **rerun the job once**; if it
  still fails, read the latest log to see if the archive itself moved.
- `Build binaries`: same — if Windows runner complains about `cargo
  build` failing, rerun once.

When `Create GitHub release` succeeds, the Release is published and
releases.json is regenerated. Verify:

```bash
gh release view windows-v<version> -R <owner>/<repo> --json assets,name,publishedAt
```

### 4. Conventional release — return to dev

The discipline requires that we leave main on the next `-dev` number so
the next feature commit does not accidentally produce a tagged build:

```bash
# All five windows Cargo.toml: bump to <next>-dev
# Cargo.lock: cargo update --workspace
git commit -m "chore(release): <next>-dev"
git push origin main
```

### 5. Bundled variant — when to invoke

The bundled variant is **only** invoked when the user asks for it
explicitly. It is not part of every release. The variant requires:

- A base version that is already released (so the bundled asset is an
  *addition* to an existing Release, not a brand new Release).
- The user agrees to ship third-party AI models under Apache-2.0
  (SenseVoice, punctuation CT-Transformer, homophone-replacer).

When the user requests bundled for an existing release:

```bash
# Verify the current main has the bundled-detection release.yml in place.
grep -c 'BUNDLED=1' .github/workflows/release.yml
grep -c 'Pre-fetch bundled models' .github/workflows/release.yml
```

If either is `0`, the workflow changes are missing. Tell the user that
the bundled workflow depends on `feat(release): bundled 变体` and
`ci(release): bundled tag 跳过 main 祖先门禁` commits being on the
release.yml the tag uses. This is exactly the situation that bit the
0.1.11 run; explain the underlying reason (CI checkout uses the tag's
release.yml, not main's).

### 6. Bundled variant — create a detached patch commit

The tag `windows-v<version>-bundled` MUST point at a commit whose
release.yml contains the bundled detection (`BUNDLED=1`,
`Pre-fetch bundled models`). If main has it but the historical commit
the tag should point at doesn't, build a cherry-pick:

```bash
git checkout -d <base-commit>     # detached HEAD at the historical 0.1.11 commit
git checkout main -- .github/workflows/release.yml
git commit --amend --no-edit       # or commit a fresh patch
git tag -fa windows-v<version>-bundled -m "字在 Zizai Windows <version> bundled" HEAD
git push origin :refs/tags/windows-v<version>-bundled
git push origin windows-v<version>-bundled
```

The patch commit does NOT need to be on main. It only needs to be
reachable from the tag. CI checkout pulls the release.yml from this
patch commit's tree.

### 7. Bundled variant — pin the model archives

The release.yml uses three archives. Their SHA-256 are pinned in
`.github/workflows/release.yml` lines around the `Pre-fetch bundled
models` block. If you bump the model version, also bump the SHA-256
constants:

| Archive | k2-fsa URL fragment | SHA-256 (as of 2026-09-24) |
|---|---|---|
| SenseVoice int8 (zh/en/ja/ko/yue) | `asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2024-07-17.tar.bz2` | `7d1efa2138a65b0b488df37f8b89e3d91a60676e416f515b952358d83dfd347e` |
| Punctuation CT-Transformer (int8) | `punctuation-models/sherpa-onnx-punct-ct-transformer-zh-en-vocab272727-2024-04-12-int8.tar.bz2` | `c0d5aa5f8eeb686032345e180bedf39319dc2e0556781c6264bcadba8328a6e1` |
| hr lexicon | `hr-files/lexicon.txt` | `978900e511bc481b8630cb6e4a573c12566fa092c366d5396e2c3823dec9dcb9` |
| hr replace.fst | `hr-files/replace.fst` | `6316a8496edf222191fd768ef08e420230097ec5076f2a2665f7d07e17051896` |

To compute a new SHA-256:

```bash
curl -L -o /tmp/x.tar.bz2 '<URL>' && sha256sum /tmp/x.tar.bz2
```

### 8. Bundled variant — CI and verification

The CI run takes ~6-7 minutes total (model download + build + Inno
package). Watch:

```bash
gh run list -R <owner>/<repo> -L 1 --json databaseId,name,headSha --jq '.[0]'
gh run watch <id> -R <owner>/<repo> --exit-status
```

On success, the new bundled exe appears as a second asset on the SAME
Release (not a new Release):

```bash
gh release view windows-v<version> -R <owner>/<repo> --json assets \
  --jq '.assets | map({name, size})'
```

You should see two exes: standard (~80 MB) and bundled (~295 MB).
`releases.json` is regenerated against the BASE tag (`windows-v<version>`)
so the official site download page surfaces both.

## Failure recovery cookbook

These are the failures observed during the 0.1.11 cycle, with the fixes
that worked.

### `Check version matches tag and tag is on main` fails

`apps/windows/server/Cargo.toml 是 0.1.10，标签是 windows-v0.1.11-bundled，先改版本号再打标签`

Cause: tag is `windows-v0.1.11-bundled` but version is `0.1.10` (you
tagged against the wrong commit), or version is `0.1.11-dev`.

Fix:
- The bundled branch in release.yml strips `-bundled` before comparing,
  so the version mismatch means the underlying Cargo.toml does not
  match. Pick the right commit.
- If version has `-dev`, you forgot to drop it before tagging. Apply
  the discipline in step 2.

### `Pre-fetch sherpa-onnx native lib` fails with "could not find native static library `sherpa-onnx-c-api`"

Cause: the Windows runner's `sherpa-onnx-sys` build.rs couldn't find
the lib. The current release.yml ships a PowerShell step that pre-fetches
the archive and sets `SHERPA_ONNX_LIB_DIR`. Rerun once — sometimes the
Swatinem cache on a fresh runner conflicts with build.rs expectations.
If still failing, the archive SHA-256 in the workflow may be stale.

### `Pre-fetch bundled models (SenseVoice / punctuation / hr)` fails with "Cannot bind argument to parameter 'Path' because it is null"

Cause: `$Repo` was used inside the PowerShell step but `$Repo` is not
a workflow-defined variable. The fix is to use `$env:GITHUB_WORKSPACE`
instead.

Fix: edit the workflow, change `$Repo` to `$env:GITHUB_WORKSPACE`. The
file is now correct as of 2026-09-24.

### Tag push succeeded but CI did not pick it up

Wait a moment — sometimes the tag push trigger takes 30-60 s. If it
still doesn't show up in `gh run list`, check `gh api
repos/<owner>/<repo>/actions/runs?event=push` to confirm the workflow
filter is matched. The release.yml filter is
`on: push: tags: ['windows-v*']` plus `'windows-v*-bundled'` (since
the bundled variant).

### Bundled run produces `BUNDLED=1` not detected

Cause: the `BUNDLED=1` is written via `echo ... >> $GITHUB_ENV` mid-step,
but GitHub Actions does not make that env var available within the
same step. The fix is to use a local bash variable and check it
inline:

```bash
BUNDLED="0"
if [ "${TAG_REF_NAME%-bundled}" != "$TAG_REF_NAME" ]; then
  BUNDLED="1"
fi
# ...later in the same step...
if [[ "$BUNDLED" == "1" ]]; then
  echo "bundled 变体：跳过 main 祖先检查"
else
  git merge-base --is-ancestor ...
fi
# At the end of the step, write to GITHUB_ENV for later steps:
if [[ "$BUNDLED" == "1" ]]; then
  echo "BUNDLED=1" >> "$GITHUB_ENV"
  ...
fi
```

This is the pattern used in the current release.yml. Do not refactor
back to "write then read in same step".

## Key references in the repo

- `docs/notes/release.md` — canonical process. Read this before every
  release; the skill encapsulates, but the doc explains the why.
- `docs/contributing.md` — commit message convention and version
  discipline.
- `docs/design/code-signing.md` — SignPath; read if the user wants to
  enable signing.
- `apps/windows/installer/build.ps1` — local packager; understand it
  before flagging a `Build binaries` failure.
- `.github/workflows/release.yml` — the workflow this skill drives.
- `.github/workflows/ci.yml` — pre-tag gate (must be green before
  pushing the tag).
- `tools/release/{data-bundle.sh,pack-model.sh,releases_json.py,publish-releases-json.sh,bump-website.sh}`
  — bundled run pulls from `data` Release for product data; the script
  names appear in release.yml so name changes there must propagate
  here.

## Hygiene rules

1. Never push a tag whose commit is dirty (`git status` clean).
2. Never push `windows-v<version>` if `apps/windows/server/Cargo.toml`
   still has `-dev` in version.
3. Never push a tag without a corresponding CHANGELOG.md entry.
4. Never push a bundled tag before the base release has shipped and is
   reachable from `gh release view windows-v<version> -R <owner>/<repo>`.
5. Never touch Cargo.lock without committing it in the same commit as
   the Cargo.toml change.
6. Always return to `-dev` immediately after a release so the working
   tree does not accidentally produce a tagged build.

## One-shot full script (use with caution)

If the user says "do a dry run / scripted full release", paste this into
an `eval` cell, with the version filled in:

```python
import subprocess, os
REPO = r'D:\qingjian'
VER  = '0.1.12'                    # <-- FILL IN
NEXT = '0.1.13'                    # <-- FILL IN
def run(args, **kw):
    r = subprocess.run(args, capture_output=True, cwd=REPO, **kw)
    print(r.returncode, r.stdout.decode('utf-8','replace')[:400])
    return r

# 1. confirm clean tree
run(['git','status','--short'])

# 2. set five Cargo.toml versions
for sub in ['server','tsf','settings','settings-egui','voice-worker']:
    p = os.path.join(REPO, f'apps/windows/{sub}/Cargo.toml')
    txt = open(p,encoding='utf-8').read()
    txt2 = txt.replace('version = "0.1.13-dev"', f'version = "{VER}"')
    open(p,'w',encoding='utf-8').write(txt2)

# 3. update lock
run(['cargo','update','--workspace'])

# 4. CHANGELOG: prepend a new section — do this in the editor manually
#    because the content must be human-written (LLM may draft, human signs off).

# 5. commit and push
run(['git','add','Cargo.lock'] + [f'apps/windows/{s}/Cargo.toml' for s in
      ['server','tsf','settings','settings-egui','voice-worker']])
run(['git','commit','-m',f'chore(release): {VER}'])
run(['git','push','origin','main'])

# 6. wait for ci.yml to be green, then:
#    gh run watch ...
#    when green:
run(['git','tag','-a',f'windows-v{VER}','-m',f'字在 Zizai Windows {VER}'])
run(['git','push','origin',f'windows-v{VER}'])

# 7. watch release.yml
#    gh run watch ...
#    when green, the GitHub Release exists.

# 8. return to -dev: same edits but bumping to NEXT-dev.
```

Stop the script before step 4 (CHANGELOG) so the user can write that
section themselves or approve the LLM draft.