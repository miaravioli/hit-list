#!/usr/bin/env python3
"""
Re-sync sqlx's stored checksum for migration 3 with the SQL currently in lib.rs.

Usage (run with the app closed):
    python3 fix_migration_checksum.py <path/to/lib.rs> <path/to/tasks.db>          # dry run
    python3 fix_migration_checksum.py <path/to/lib.rs> <path/to/tasks.db> --apply  # write it
"""
import hashlib
import re
import sqlite3
import sys

VERSION = 3
EXPECTED_COLUMNS = ("position", "highlighted")  # what migration 3 should have created


def rust_unescape(s: str) -> str:
    """Turn the body of a Rust "..." string literal into the actual string."""
    s = re.sub(r"\\\n\s*", "", s)  # backslash line continuations
    esc = {'"': '"', "\\": "\\", "n": "\n", "t": "\t", "r": "\r", "'": "'", "0": "\0"}
    out, i = [], 0
    while i < len(s):
        if s[i] == "\\" and i + 1 < len(s) and s[i + 1] in esc:
            out.append(esc[s[i + 1]])
            i += 2
        else:
            out.append(s[i])
            i += 1
    return "".join(out)


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    lib_rs, db_path = sys.argv[1], sys.argv[2]
    apply = "--apply" in sys.argv

    # rustc normalizes CRLF to LF inside string literals, so do the same
    src = open(lib_rs, encoding="utf-8").read().replace("\r\n", "\n")
    m = re.search(
        r'version:\s*%d\s*,.*?sql:\s*"((?:[^"\\]|\\.)*)"' % VERSION, src, re.S
    )
    if not m:
        sys.exit(f"Couldn't find a string-literal `sql:` for migration {VERSION} in {lib_rs}")
    sql = rust_unescape(m.group(1))
    new_checksum = hashlib.sha384(sql.encode("utf-8")).digest()

    con = sqlite3.connect(db_path)
    row = con.execute(
        "SELECT description, success, checksum FROM _sqlx_migrations WHERE version = ?",
        (VERSION,),
    ).fetchone()
    if not row:
        sys.exit(f"Migration {VERSION} isn't recorded in this database - nothing to fix.")
    description, success, old_checksum = row

    print(f"Applied description : {description}")
    print(f"Applied successfully: {bool(success)}")
    print(f"Stored checksum     : {bytes(old_checksum).hex()[:24]}...")
    print(f"Current checksum    : {new_checksum.hex()[:24]}...")

    if bytes(old_checksum) == new_checksum:
        print("\nThey already match - migration 3 isn't the problem.")
        return

    # Make sure the database really has what the current migration 3 describes
    cols = [r[1] for r in con.execute("PRAGMA table_info(task)")]
    missing = [c for c in EXPECTED_COLUMNS if c not in cols]
    if missing:
        sys.exit(
            f"\nThe task table is missing {missing}. The applied migration 3 differs from "
            "the current one in substance, not just wording. Don't sync the checksum - "
            "move the missing part into a new migration 4 instead."
        )
    nulls = con.execute("SELECT COUNT(*) FROM task WHERE position IS NULL").fetchone()[0]
    if nulls:
        print(
            f"\nWarning: {nulls} task rows have a NULL position, so the UPDATE line may "
            "never have run. Consider putting `UPDATE task SET position = id WHERE "
            "position IS NULL;` in migration 4."
        )

    if not apply:
        print("\nSchema looks right. Dry run only - re-run with --apply to update the checksum.")
        return

    con.execute(
        "UPDATE _sqlx_migrations SET checksum = ? WHERE version = ?",
        (new_checksum, VERSION),
    )
    con.commit()
    print("\nChecksum updated. Dev mode should start cleanly now; rebuild the app too.")


if __name__ == "__main__":
    main()
