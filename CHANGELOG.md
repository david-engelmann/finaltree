# Changelog

All notable changes to finaltree are recorded in this file.

## Unreleased

- Align `Cargo.toml` package metadata (`version = "0.21.0"`, `rust-version = "1.80"`, `repository`, `homepage`, `documentation`, `readme`, `keywords`, `categories`, `binstall`) and `.crate` archive `exclude` rules.
- Add storefront installation, quickstart, and CI merge-candidate provenance sections in `README.md`, full six-document mdBook handbook under `docs/` and `book/`, bespoke dark and light SVG architecture diagrams under `docs/media/`, rate-limit-safe checksum-verifying `install.sh`, `SECURITY.md`, `CONTRIBUTING.md`, `CITATION.cff`, `deny.toml`, `.editorconfig`, `.gitattributes`, GitHub issue templates, and `.github/workflows/{release.yml,docs.yml,clean-install.yml}`.

## 0.21.0 on 2026-11-27

- Complete 23 closed verification rounds covering working-tree SHA-256 fingerprinting (`HEAD`, tracked working-tree and index blobs, untracked blobs, dirty classifications, `--generated` artifact globs, recursive submodule state, executable `100755` and symlink `120000` modes, and special-file/directory-mode handling), tamper-evident `finaltree-stamped-log/1` command execution, and member-by-member `verify` diffing across `fingerprint`, `stamp`, and `verify`.
