# Changelog & release flow

This project uses [git-cliff](https://git-cliff.org/) to generate
`CHANGELOG.md` at release time. The `[Unreleased]` section is **not**
maintained in-tree between releases — pending changes are viewed on
demand instead of being hand-edited per PR.

## Previewing pending changes

```bash
just changelog-preview
```

This runs `git cliff --unreleased` and prints the generated section to
stdout without modifying any files. Use it before cutting a release to
see what the next section will look like.

## Cutting a release

The release script (project-specific; not provided by the stub) should:

1. Bump the version in whatever files hold it (`package.json`,
   `Cargo.toml`, `pyproject.toml`, `Dockerfile`, etc.).
2. Run git-cliff to prepend the new section to `CHANGELOG.md`:

   ```bash
   git-cliff --unreleased --tag "v${NEW_VERSION}" --prepend CHANGELOG.md
   ```

   - `--unreleased` processes only commits since the last tag.
   - `--tag` tells git-cliff to label those commits with the new
     version (the tag doesn't exist yet at this point).
   - `--prepend` inserts the new section above existing entries,
     preserving prior history exactly as written.

3. Commit the version bump + CHANGELOG update as `release: vX.Y.Z` and
   create the annotated tag `vX.Y.Z`.

4. Push and publish (`git push --follow-tags`, then
   `gh release create vX.Y.Z --generate-notes` or equivalent).

## Why no `[Unreleased]` in-tree?

Maintaining a hand-edited `[Unreleased]` section creates merge conflicts
with concurrent PRs and adds a per-PR contributor task. For projects
without high contributor concurrency, regenerating from commit history
at release time is simpler and just as accurate, and `make
changelog-preview` covers the "what's pending?" use case on demand.
