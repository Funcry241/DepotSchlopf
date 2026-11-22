"""Rank frozen DepotSchlopf pairs through an explicitly local embedding server."""
import argparse
import hashlib
import json
import math
from pathlib import Path
from urllib.request import HTTPRedirectHandler, ProxyHandler, Request, build_opener

FORMAT = "depot-neuro-textprobe-v1"
INSTRUCTION = "Retrieve evidence relevant to assessing this investment thesis at its stated cutoff."
MODEL_DIMENSION = 1024


class NoRedirects(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


OPENER = build_opener(ProxyHandler({}), NoRedirects())


def vectors(texts, port, dimension):
    aus = {}
    for start in range(0, len(texts), 16):
        block = texts[start:start + 16]
        data = json.dumps({"input": block, "model": "local"}).encode()
        request = Request(f"http://127.0.0.1:{port}/v1/embeddings", data,
                          headers={"Content-Type": "application/json"})
        with OPENER.open(request, timeout=90) as reply:
            zeilen = sorted(json.load(reply)["data"], key=lambda row: row["index"])
        if [z["index"] for z in zeilen] != list(range(len(block))):
            raise ValueError("Der lokale Server lieferte eine andere Textmenge.")
        for key, row in zip(block, zeilen):
            werte = row["embedding"]
            if len(werte) != MODEL_DIMENSION or not all(map(math.isfinite, werte)):
                raise ValueError("Erwartet werden 1.024 endliche Vektorkomponenten.")
            werte = werte[:dimension]
            norm = math.sqrt(sum(x * x for x in werte))
            if not math.isfinite(norm) or norm == 0:
                raise ValueError("Die gewählten Vektorkomponenten lassen sich nicht normalisieren.")
            aus[key] = [x / norm for x in werte]
    return aus


def rangantwort(rohdaten, port, dimension=MODEL_DIMENSION):
    eingabe = json.loads(rohdaten)
    if eingabe["format"] != FORMAT or not eingabe["rows"]:
        raise ValueError("Unbekanntes oder leeres Textprobe-Format.")
    paare = [
        (z["id"], f"Instruct: {INSTRUCTION}\nQuery:{z['these']}", z["beleg"])
        for z in eingabe["rows"]
    ]
    if len({p[0] for p in paare}) != len(paare):
        raise ValueError("Doppelte Fallkennung in den Eingaben.")
    gespeicherte_vektoren = vectors(list(dict.fromkeys(
        text for _, these, beleg in paare for text in (these, beleg)
    )), port, dimension)
    return {
        "format": FORMAT,
        "input_sha256": hashlib.sha256(rohdaten).hexdigest(),
        "dimension": dimension,
        "scores": [
            {"id": fall, "score": math.fsum(a * b for a, b in
                zip(gespeicherte_vektoren[these], gespeicherte_vektoren[beleg]))}
            for fall, these, beleg in paare
        ],
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("ordner", type=Path)
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--dimension", type=int, choices=(512, MODEL_DIMENSION), default=MODEL_DIMENSION)
    args = parser.parse_args()
    if not 1 <= args.port <= 65535:
        parser.error("Der lokale Server-Port liegt außerhalb des gültigen Bereichs.")
    eingabe = args.ordner / "textprobe-eingabe.json"
    ziel = args.ordner / "textprobe-antwort.json"
    if eingabe.stat().st_size > 4_000_000:
        raise ValueError("Die lokale Textprobe überschreitet die unterstützte Dateigröße.")
    antwort = rangantwort(eingabe.read_bytes(), args.port, args.dimension)
    zwischenstand = ziel.with_suffix(".tmp")
    zwischenstand.write_text(json.dumps(antwort, allow_nan=False) + "\n", encoding="utf-8")
    zwischenstand.replace(ziel)
    print(f"{len(antwort['scores'])} lokale Rangwerte mit {args.dimension} Dimensionen in {ziel.name} gespeichert.")


if __name__ == "__main__":
    main()
