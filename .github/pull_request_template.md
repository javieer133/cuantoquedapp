## Summary

<!-- What changes and why. Link the issue it closes: "Closes #123". -->

## How it was tested

<!-- Tests added, and what you checked by hand (macOS, Windows, a screenshot of the panel). -->

## Checklist

- [ ] The PR targets `develop` (only release PRs target `main`).
- [ ] `scripts/check.sh` is green (tests, clippy for macOS and Windows, fmt).
- [ ] New interface texts are in `locales/app.yml` in English and Spanish.
- [ ] If the README changed, `README.md` and `README.es.md` both changed.
- [ ] No `unwrap`/`panic` when reading data, and nothing prints conversation content.
