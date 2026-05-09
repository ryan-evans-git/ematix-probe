# Release process

How to cut a `vX.Y.Z` release of `ematix-probe` to PyPI. The
release pipeline lives in [`.github/workflows/release.yml`](../.github/workflows/release.yml);
this doc is the manual runbook around it.

## Prerequisites (one-time)

1. **PyPI trusted-publisher record.** On
   <https://pypi.org/manage/project/ematix-probe/settings/publishing/>
   add a *pending publisher*:
   - Owner: `ryan-evans-git`
   - Repository: `ematix-probe`
   - Workflow: `release.yml`
   - Environment: `pypi`

   After the first publish, the record becomes permanent. Without
   this, the `publish` job in `release.yml` will fail at OIDC
   token-mint time.
2. **GitHub `pypi` environment.** Settings → Environments →
   `pypi`. No secrets needed (trusted publishing handles auth);
   the environment exists to scope OIDC token issuance.

## Per-release checklist

### 1. Pre-flight on `main`

- [ ] `cargo test --workspace` green locally.
- [ ] `pytest` green locally.
- [ ] `coverage run -m pytest && coverage report --fail-under=90`
      passes.
- [ ] CI green on the latest `main` SHA (the release workflow
      gates on this).
- [ ] CHANGELOG `## [Unreleased]` block describes everything in
      the release.

### 2. Bump the version

Three places, in lockstep:

- `pyproject.toml`: `version = "X.Y.Z"` (drop the `.dev0` suffix).
- `Cargo.toml` workspace `[workspace.package] version = "X.Y.Z"`
  (covers every crate via `version.workspace = true`).
- `python/ematix_probe/__init__.py` re-exports
  `_core.__version__`; the Rust side reads from Cargo, so no
  Python edit is required *if* the Cargo bump is done.

Verify the round-trip:

```sh
maturin develop
python -c "import ematix_probe; print(ematix_probe.__version__)"
# → X.Y.Z
```

### 3. Promote the CHANGELOG block

Move everything under `## [Unreleased]` to a new section:

```markdown
## [X.Y.Z] - YYYY-MM-DD
```

Leave a fresh empty `## [Unreleased]` block at the top for the
next cycle.

### 4. Dry-run the build matrix (recommended)

Before tagging, fire a manual `workflow_dispatch` run of
`release.yml` from the Actions tab. This builds every wheel in
the matrix (Linux × {3.11, 3.12, 3.13} + macOS aarch64 × same +
sdist) but skips the `publish` job because the trigger is not a
tag push. Inspect the run-summary artifacts to confirm the
wheels look right.

Two things to watch:
- Wheel filenames carry the new version (`ematix_probe-X.Y.Z-...whl`).
- Sdist tarball is non-empty and includes the `crates/` tree (so
  `pip install --no-binary :all: ematix-probe` builds from
  source on platforms outside the wheel matrix).

### 5. Commit + tag + push

```sh
git add pyproject.toml Cargo.toml CHANGELOG.md
git commit -m "chore: release vX.Y.Z"
git tag vX.Y.Z
git push origin main
git push origin vX.Y.Z
```

The tag push triggers `release.yml`, which:

1. **Waits for `ci.yml`** on the same SHA (30-min cap). Tag and
   main pushes go through together — `ci.yml` is in flight by
   the time the wait loop polls.
2. **Builds wheels** for Linux × {3.11, 3.12, 3.13} +
   macOS aarch64 × same.
3. **Builds the sdist** once on Linux.
4. **Publishes to PyPI** via trusted publishing (no API token
   needed; the OIDC claim from the `pypi` environment is
   minted into a short-lived PyPI token).

### 6. Post-publish verification

Wait ~2 min for the PyPI CDN to refresh, then verify on a
clean machine / fresh venv:

```sh
python -m venv /tmp/verify-ematix-probe
/tmp/verify-ematix-probe/bin/pip install ematix-probe==X.Y.Z
/tmp/verify-ematix-probe/bin/ematix-probe doctor
# → all checks [OK]
```

### 7. GitHub release

```sh
# Pull the section you just promoted into the release notes body.
gh release create vX.Y.Z --title "vX.Y.Z" --notes-file <(
  awk '/^## \[X\.Y\.Z\]/,/^## \[/{print}' CHANGELOG.md
    | sed '$d'   # strip the trailing next-section header
)
```

…or paste the relevant CHANGELOG section into the GitHub UI by
hand.

## Wheel-build matrix philosophy

Documented inline at the top of `release.yml`. Summary:

- **6 wheels per release**: linux-x86_64 × {3.11, 3.12, 3.13}
  and macos-aarch64 × {3.11, 3.12, 3.13}.
- **Linux**: `manylinux_2_28` container — wheels run on every
  glibc-2.28+ system.
- **macOS**: aarch64 (Apple Silicon) only. Apple Silicon covers
  every Mac shipped since Nov 2020. Intel-Mac users install the
  sdist (one extra step — needs Rust locally).
- **Windows**: not built in v0.1. Add a Windows job here when
  the Postgres / DuckDB / object_store stack is verified
  against the MSVC toolchain.
- **sdist**: built once on Linux, includes the full Rust
  workspace so `pip install --no-binary` works on any platform.

## TestPyPI

Not currently wired into `release.yml`. If you want a TestPyPI
rehearsal before a real release, add a manual step that builds
locally and uploads with an explicit `--repository testpypi`:

```sh
maturin build --release --manylinux 2_28
python -m twine upload --repository testpypi target/wheels/*.whl
pip install --index-url https://test.pypi.org/simple/ \
            --extra-index-url https://pypi.org/simple/ \
            ematix-probe==X.Y.Z.devN
```

The test repository is forgiving of broken wheels — useful for
catching metadata / classifier mistakes before they hit real
PyPI. Real PyPI rejects re-uploads of a version, so a TestPyPI
dry-run on a `.devN` suffix avoids burning a real version
number on a typo.
