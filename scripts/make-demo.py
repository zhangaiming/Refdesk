"""Create an isolated merge, linked worktree and Excel diff demo."""
import os
from pathlib import Path
import subprocess
import tempfile

from openpyxl import Workbook


def git(root, *args):
    env = {key: value for key, value in os.environ.items()
           if not key.upper().startswith("GIT_")}
    env.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull,
               GIT_ATTR_NOSYSTEM="1", GIT_TEMPLATE_DIR="", GIT_TERMINAL_PROMPT="0")
    subprocess.run(
        ["git", "-C", str(root), "-c", f"core.attributesFile={os.devnull}",
         "-c", f"core.excludesFile={os.devnull}", *args],
        env=env, check=True,
    )


def workbook(path, changed):
    book = Workbook()
    settings = book.active
    settings.title = "Balance"
    settings.append(["Unit", "Power", "Forecast"])
    settings.append(["Scout", 18 if changed else 12, "=B2*3" if changed else "=B2*2"])
    settings.append(["Guardian", 24, "=B3*2"])
    if changed:
        book.create_sheet("Notes")["B4"] = "Reviewed"
    book.save(path)


def main():
    folder = Path(tempfile.mkdtemp(prefix="Refdesk demo "))
    root = folder / "lighthouse"
    review = folder / "linked review"
    root.mkdir()
    git(root, "init", "--initial-branch=main")
    git(root, "config", "user.name", "Lin Chen")
    git(root, "config", "user.email", "lin@example.invalid")
    git(root, "config", "commit.gpgSign", "false")
    git(root, "config", "core.autocrlf", "false")
    (root / "fixtures").mkdir()
    (root / "README.md").write_text("# Lighthouse\nWeather and harbor status.\n", encoding="utf-8")
    table = root / "fixtures" / "Balance Sheet.xlsx"
    workbook(table, False)
    git(root, "add", ".")
    git(root, "commit", "-m", "Initialize harbor dashboard")
    git(root, "tag", "v0.1.0")
    git(root, "switch", "-c", "feature/forecast")
    (root / "forecast.txt").write_text("Visibility: clear\nSwell: 0.9 m\n", encoding="utf-8")
    git(root, "add", ".")
    git(root, "commit", "-m", "Show swell and visibility in the forecast")
    git(root, "switch", "main")
    (root / "README.md").write_text("# Lighthouse\nWeather, harbor and tide status.\n", encoding="utf-8")
    git(root, "add", ".")
    git(root, "commit", "-m", "Document tide status and update onboarding")
    git(root, "merge", "--no-ff", "feature/forecast", "-m", "Merge feature/forecast")
    git(root, "worktree", "add", "-b", "review/spreadsheet", str(review), "HEAD~1")
    (review / "review.txt").write_text("Checklist: preview graph and spreadsheet.\n", encoding="utf-8")
    git(review, "add", ".")
    git(review, "commit", "-m", "Record spreadsheet review")
    workbook(table, True)
    (root / "README.md").write_text("# Lighthouse\nWeather, tide and swell forecasts.\n\nUpdated every 20 min.\n", encoding="utf-8")
    (root / "notes.txt").write_text("Local notes to try staging.\n", encoding="utf-8")
    print(f"Main: {root}\nLinked: {review}", flush=True)


if __name__ == "__main__":
    main()