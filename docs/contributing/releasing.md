# Cutting a Release

The runbook for shipping a `framec` release. Follow it top to bottom.

This is the **operational** guide — the exact commands and the
order to run them. The **normative** process (rationale, CI gates,
drift-detection policy) lives in
[RFC-0031](../rfcs/rfc-0031.md); when this guide and RFC-0031
disagree, RFC-0031 wins and this file is the one to fix.

The release process is a four-layer discipline (RFC-0031): **RC
validation → merge gate → release sequence → drift detection**.
Layers 1 and 3 are manual and live here. Layer 2 (per-push CI) and
Layer 4 (nightly) are automated; this guide tells you how to read
them, not run them by hand.

---

## 0. Decide the version

Semver, per RFC-0031 § "Semver policy":

| Bump | When | Example |
|---|---|---|
| **Major** `X.0.0` | User-facing source incompatibility — a previously-valid Frame source no longer compiles, or an RFC hard-cut removes syntax (RFC-0013 `@@target`, RFC-0015 factory, RFC-0024 `@@import`). **Requires a migration guide.** | `4.0.0` |
| **Minor** `X.Y.0` | New features / language constructs / backends. Existing valid source still compiles identically. | `4.2.0` |
| **Patch** `X.Y.Z` | Bug fixes, semantically-equivalent codegen changes, perf, docs. | `4.2.1` |

Pre-releases use `vX.Y.Z-rc.N` (the hyphen makes CI auto-flag the
GitHub release as *pre-release*). Promote `rc.N` → `vX.Y.Z` only
once Layers 1–3 are clean.

---

## 1. RC validation (the tag bar)

Every command below must return a **clean exit**. No exceptions for
"known flakes" — if a check flakes, re-run it; if it flakes twice,
the flake is a release blocker, not a personality trait
(RFC-0031 § "RC bar").

```bash
# ── framec local ────────────────────────────────────────────────
cd /path/to/framec
cargo build --release --locked --bin framec  # exactly what release.yml builds; needs a current committed Cargo.lock
cargo test --release                       # unit + RFC-0027 snapshot tests
cargo clippy --release --all-targets -- -D warnings
cargo fmt --check
python3 scripts/validate_doc_samples.py    # every runnable docs/ sample compiles + runs
cargo package -p framec                     # dry-run the crates.io package (catches metadata/file issues pre-publish)

# ── 17-backend differential matrix ──────────────────────────────
cd /path/to/framec-test-env/docker
make test                                  # expect: "17 languages clean, 0 with failures"

# ── fuzz smoke (21 phases × backends, ~2-4 min) ─────────────────
cd /path/to/framec-test-env/fuzz
export FRAMEC=/path/to/framec/target/release/framec   # absolute path — see gotcha below
./run_all.sh --tier=smoke                  # expect: 0 fails aggregated per lang
```

For a release (as opposed to a routine merge), also run the **full**
fuzz tier — it catches edition/no_std/runtime regressions the smoke
tier and the matrix miss (this is how the edition-2015 and
no_std-macro regressions, FRAMEC_BUGS #31/#33, were caught):

```bash
cd /path/to/framec-test-env/fuzz
export FRAMEC=/path/to/framec/target/release/framec
./run_all.sh --tier=full                   # ~20-50 min; 0 failures attributable to your changes
```

> **Known non-defects** that may appear in the full fuzz run and do
> **not** block a release (verify the signature matches before
> waving one through):
> - **operations / any phase, a lone `cargo build` timeout** — the
>   diff-harness compiles each Rust case in a fresh cargo project;
>   a cold build under load can exceed the 180 s cap. Re-run the
>   phase; it passes warm.
> - **async / java `pass=9/12`** — the documented Java `init()`-no-op
>   limitation (`self.x = await op()` in a `$>` enter doesn't run
>   during Java construction). Tracked in `_scratch/FRAMEC_BUGS.md`.

> **FRAMEC env gotcha.** The fuzz harness defaults `FRAMEC` to
> `../../framepiler/target/release/framec`. The shell's working
> directory resets between invocations, and a `framepiler` symlink
> may not exist — so **always `export FRAMEC=` with the absolute
> path to the framec build you intend to test**, and confirm it's
> the one you just built (`framec <x>.frs | grep <a-recent-change>`).

### RC discipline (RFC-0031)

- Verify each open roadmap item against current code/git before
  acting on it — auto-memory and `_scratch/roadmap.md` decay faster
  than the code. When in doubt, run the command, not the
  recollection.
- If a stale claim surfaces (an already-shipped RFC cited as a
  blocker, a roadmap `- [ ]` that's actually done), close it
  immediately and continue.

---

## 2. Read the merge gate (CI)

Every push to `main` and every PR head runs three parallel jobs
(~4 min total): `framec-local`, `matrix`, `fuzz-smoke`. Merge is
blocked if any fail. There is **no "merge anyway"** without a
written exception in the PR naming the RFC or roadmap task that
tracks the regression.

These run via `.github/workflows/ci.yml`, `matrix-smoke.yml`, and
`fuzz-smoke.yml`; `nightly.yml` and `quarterly-audit.yml` cover
Layer 4. Before tagging, confirm the `main` you're tagging is green
on all three per-push jobs.

---

## 3. Bump the version

Three touchpoints. Grep for the old version string to be sure you
caught them all (`git grep -n "<old-version>"`):

| File | What to change |
|---|---|
| `Cargo.toml` (workspace root) | `[workspace.package] version = "X.Y.Z"` — the single source of truth; the crate inherits it via `version.workspace = true`. |
| `README.md` | the `![Version]` shields.io badge. |
| `CHANGELOG.md` | promote the `[Unreleased]` section to `[X.Y.Z] - <date>` and start a fresh `[Unreleased]`. |

Commit the bump on its own (`chore(release): X.Y.Z`) so the tag
lands on a clean, self-describing commit.

---

## 4. Changelog & migration notes

- **`CHANGELOG.md`** — author the entry from the
  commits-since-last-tag, but **edit for clarity; never dump raw
  `git log`**. Sections: *Added*, *Fixed*, *Changed/Breaking*,
  *Deprecated/Removed*. Reference RFCs and `FRAMEC_BUGS` issue
  numbers (e.g. "Removed `@@codegen` (RFC-0032)", "Fixed no_std
  Rust paths (#31/#33)").

  ```bash
  git log --oneline <last-tag>..HEAD     # raw material, then edit
  ```

- **Migration guide** — required for any **breaking change** (any
  Major bump, and Minor bumps that change wire formats or remove
  syntax). Add `docs/migration/<from>_to_<to>.md` following the
  shape of [`4.1_to_4.2.md`](../migration/4.1_to_4.2.md): a
  break-by-break table (symptom → fix scope) plus a worked
  before/after for each. Link it from the release notes.

---

## 5. Tag & ship

Once Layer 1 is clean, CI is green, and the bump + changelog are
committed and pushed to `main`:

```bash
git tag -a vX.Y.Z -m "release X.Y.Z"
git push origin vX.Y.Z          # triggers the GitHub Actions release workflow
```

Pushing the tag triggers `.github/workflows/release.yml` — it runs
`cargo build --release --locked --target <T> --bin framec` for each
platform, attests provenance, generates `SHA256SUMS`, and creates
the GitHub Release with the binaries attached. `-rc.N` tags
auto-flag as pre-release. For a release candidate, tag
`vX.Y.Z-rc.N`; promote to `vX.Y.Z` after the monitoring window.

**Never `git push --force` a tag.** If a tag must move, delete it
and re-tag with the next version number.

### 5a. Publish to crates.io (manual)

`framec` is a published crate, and **`cargo publish` is deliberately
NOT in `release.yml`** — it stays under direct human control (see
the comment at the top of the workflow; automation is gated on a
provisioned `CARGO_REGISTRY_TOKEN`). After the GitHub release is up
and you've smoke-checked a downloaded binary, publish the crate by
hand from the tagged commit:

```bash
git checkout vX.Y.Z            # publish from exactly what you tagged
cargo publish -p framec --dry-run   # final gate — packages + verifies the build
cargo publish -p framec             # irreversible: a version can be yanked but never overwritten
```

`cargo publish` is **one-way** — you cannot re-publish a version
number. If a published version is broken, `cargo yank` it and ship
`vX.Y.(Z+1)`. This is why the `--dry-run` + the RC bar matter.

---

## 6. Post-release

1. **Announce** — once the GitHub release page is up, link it from
   any user-facing channel (org README release callout, status
   page).
2. **Monitor 24 h** — watch GitHub issues and user channels for
   regression reports. Triage immediately; don't let a report sit
   until "next release."
3. **Rollback**, if a regression surfaces:
   - *Patch*: cut a fast-follow `vX.Y.(Z+1)` with the fix and a
     one-line changelog entry pointing at the regression.
   - *Minor/major with shipped consumers*: per-decision — usually
     a fast-follow fix is better than yanking a published asset.
     Document the affected version in the changelog + release
     notes.

---

## 7. Drift detection (between releases)

Automated (RFC-0031 Layer 4), but know what feeds the next release:

- **Nightly** runs `make test` (matrix full) + `./run_all.sh
  --tier=full` (~50 min). A failure files a `nightly-regression`
  issue — that issue is the next workday's prompt: *what changed
  yesterday?* Don't tag on top of an open `nightly-regression`.
- **Stale-roadmap audit** (quarterly / pre-release) — verify every
  `- [ ]` in `_scratch/roadmap.md` against current state; close
  what's silently been done.
- **Auto-memory hygiene** — treat memory snapshots as hypotheses;
  when one goes stale, fix the memory file *and* its `MEMORY.md`
  index line in the same pass.

---

## Quick checklist

```
[ ] Decide version (semver)
[ ] Layer 1 RC validation — all clean (incl. cargo build --release --locked,
    cargo package, and full fuzz for a release)
[ ] CI green on the main you're tagging
[ ] Bump version: Cargo.toml + README badge + CHANGELOG
[ ] CHANGELOG entry written (edited, not git-log dump) — complete, covers all user-visible changes
[ ] Migration guide added (if breaking)
[ ] Commit bump, push main
[ ] git tag -a vX.Y.Z + push tag  → release.yml builds binaries + GitHub Release
[ ] cargo publish -p framec  (manual; --dry-run first; one-way)
[ ] Announce + monitor 24 h
```

See also: [RFC-0031](../rfcs/rfc-0031.md) (normative process),
[`testing.md`](testing.md) (the test layers in detail),
[`../migration/`](../migration/) (migration-guide examples).
