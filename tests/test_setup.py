"""Setup obligation tests for finaltree."""

from pathlib import Path

from repo_factory import make_repo

REPO_ROOT = Path(__file__).resolve().parents[1]


def test_seed_files_present():
    readme = (REPO_ROOT / "README.md").read_text(encoding="utf-8")
    licence = (REPO_ROOT / "LICENSE").read_text(encoding="utf-8")
    gitignore = (REPO_ROOT / ".gitignore").read_text(encoding="utf-8")
    assert "Finaltree asks what it ran on" in readme
    assert "## Licence" in readme
    assert "Permission is hereby granted" in licence
    assert "__pycache__/" in gitignore


def test_agents_md_stands_from_the_canonical_template():
    agents = (REPO_ROOT / "AGENTS.md").read_text(encoding="utf-8")
    marker = "<!-- repo annex below this line -->"
    assert marker in agents
    canonical_part, annex = agents.split(marker, 1)
    assert "39 repo storefront perfection program" in canonical_part
    assert "This annex is finaltree's alone" in annex
    assert "Finaltree asks what a gate ran on" in annex
    sentences = [
        "This annex is finaltree's alone",
        "Finaltree asks what a gate ran on",
        "Finaltree fingerprints the exact tree a gate ran on",
        "Fingerprints are computed from a local working tree with git only",
        "Fixtures are synthetic repos built in the tests",
        "The local gate is python -m pytest once the suite exists",
        "The closed round count stands in the README and moves only at a closure",
    ]
    positions = [annex.index(sentence) for sentence in sentences]
    assert positions == sorted(positions)


def test_readme_records_the_closed_round_count():
    readme = (REPO_ROOT / "README.md").read_text(encoding="utf-8")
    assert "Closed rounds stand at 1 of 100" in readme


def test_readme_states_the_limits_of_the_fingerprint():
    readme = (REPO_ROOT / "README.md").read_text(encoding="utf-8")
    assert "stand outside the fingerprint entirely" in readme
    assert "belongs outside the tree it stamps" in readme


def test_fixtures_are_synthetic_repos(tmp_path):
    root = make_repo(tmp_path)
    assert (root / ".git").is_dir()
    assert tmp_path in root.parents
    assert (root / "a.txt").read_text(encoding="utf-8") == "alpha\n"
