"""Entscheidung und Sammelprüfung für die Windows-Update-Gegenprobe."""
import argparse
import os
import subprocess


def update_noetig(ereignis, pfade):
    if ereignis == "push":
        return True
    if ereignis != "pull_request":
        raise ValueError(f"Unbekanntes Ereignis: {ereignis}")
    return any(
        pfad != "kodex/aenderung.txt"
        and (
            pfad in {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml"}
            or pfad.startswith(("crates/depot_lauf/", "crates/depot_kodex/", "kodex/", ".github/"))
        )
        for pfad in pfade
    )


def windows_gruen(filter_ergebnis, noetig, gate, update):
    return filter_ergebnis == gate == "success" and (
        (noetig == "true" and update == "success")
        or (noetig == "false" and update == "skipped")
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    aktionen = parser.add_subparsers(dest="aktion", required=True)
    filter_parser = aktionen.add_parser("filter")
    filter_parser.add_argument("ereignis", choices=("pull_request", "push"))
    filter_parser.add_argument("basis", nargs="?", default="")
    filter_parser.add_argument("head", nargs="?", default="")
    aktionen.add_parser("sammelcheck")
    args = parser.parse_args()
    if args.aktion == "filter":
        pfade = []
        if args.ereignis == "pull_request":
            if not (args.basis and args.head):
                parser.error("Für einen PR sind Basis und Head erforderlich.")
            diff = subprocess.check_output(
                ["git", "diff", "--name-only", "--no-renames", "-z", f"{args.basis}...{args.head}", "--"]
            )
            pfade = [os.fsdecode(pfad) for pfad in diff.split(b"\0") if pfad]
        print("update=" + ("true" if update_noetig(args.ereignis, pfade) else "false"))
        return 0

    filter_ergebnis = os.environ["FILTER"]
    noetig = os.environ["UPDATE_NOETIG"]
    gate = os.environ["GATE"]
    update = os.environ["UPDATE"]
    print(f"Windows-Filter: {filter_ergebnis} (Update nötig: {noetig})")
    print(f"Windows-Gate: {gate}\nWindows-Update: {update}")
    return 0 if windows_gruen(filter_ergebnis, noetig, gate, update) else 1


if __name__ == "__main__":
    raise SystemExit(main())
