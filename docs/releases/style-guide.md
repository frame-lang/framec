---
title: Style Guide
parent: Release Notes
nav_order: 0
---

# Release Notes — Style Guide

How to write a page in this section. Keep pages **skimmable and why-focused**;
[`CHANGELOG.md`](https://github.com/frame-lang/framec/blob/main/CHANGELOG.md)
remains the exhaustive, line-level record — release pages summarize and link to
it, they don't replace it.

## File & front matter

One page per **published** release at `docs/releases/X.Y.Z.md`:

```yaml
---
title: "vX.Y.Z"
parent: Release Notes
nav_order: <next integer, one higher than the current newest>
---
```

- The section sets `child_nav_order: reversed`, so the highest `nav_order`
  shows first — **new releases never require renumbering**.
- Skip release-candidate tags (`*-rcN`) and formatting-only point releases with
  no CHANGELOG entry.

## Page structure

1. **`# vX.Y.Z`** heading.
2. A one-line meta line:
   `*Released YYYY-MM-DD · [GitHub release](…) · [CHANGELOG](…)*`
3. **One short paragraph** — the headline in plain language: what changed and
   who's affected.
4. **`## Highlights`** — 3–6 bullets. Lead each with a **bold subject**, then
   one sentence of *what + why*. Draw from the `## [X.Y.Z]` CHANGELOG section;
   don't copy it wholesale.
5. **`## Breaking changes`** *(only if any)* — one bullet per break with its
   error code and the one-line fix.
6. **`## Action required`** *(optional)* — concrete upgrade steps, if short.

## Migration content

Breaking-change walk-throughs live **with the release that introduced them**, as
a child page of that release:

- File: `docs/releases/X.Y.Z-migration.md`
- Front matter: `parent: "vX.Y.Z"`, `grand_parent: "Release Notes"`
- Mark the release page's parent `has_children: true`, and link to the
  migration page from its **Breaking changes** section.

See [v4.2.0](4.2.0.md) and its [Migration guide](4.2.0-migration.md) for a
worked example.

## Tone

- Write for someone deciding **whether and how to upgrade**, not for a commit
  log.
- Prefer *why it changed* over *what line changed*.
- Link out (GitHub release, CHANGELOG, RFCs) rather than duplicating detail.
- Plain language over jargon; expand an RFC number on first mention.
