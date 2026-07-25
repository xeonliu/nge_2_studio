from __future__ import annotations

import argparse
import contextlib
import json
import os
import re
import sys
import tempfile
from pathlib import Path
from typing import Any

from .errors import ConversionError
from .gltf import export_hob
from .hgar import HgarArchive, HgarEntry


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Convert HGOB/HGMS resources in an HGAR to glTF 2.0"
    )
    parser.add_argument("input", type=Path, help="input HGAR .har")
    parser.add_argument("--output", type=Path, required=True, help="output directory")
    parser.add_argument("--hob", help="convert one HGOB by name, decoded ID, or typed resource key")
    parser.add_argument("--format", choices=("glb", "gltf"), default="glb")
    parser.add_argument("--skip-unsupported", action="store_true")
    parser.add_argument("--native-coordinates", action="store_true")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    report: dict[str, Any] = {
        "input": str(args.input),
        "format": args.format,
        "nativeCoordinates": args.native_coordinates,
        "models": [],
        "summary": {"succeeded": 0, "failed": 0},
    }
    try:
        args.output.mkdir(parents=True, exist_ok=True)
        archive = HgarArchive.from_file(args.input)
        candidates = [entry for entry in archive.entries if entry.signature == b"HGOB"]
        if args.hob:
            candidates = _select(candidates, args.hob)
            if not candidates:
                raise ConversionError(f"no HGOB matches {args.hob!r}")
    except ConversionError as error:
        report["archiveError"] = error.as_report()
        _write_report(args.output, report)
        print(error, file=sys.stderr)
        return 2
    except OSError as error:
        report["archiveError"] = {"message": str(error)}
        with contextlib.suppress(OSError):
            _write_report(args.output, report)
        print(error, file=sys.stderr)
        return 2

    for entry in candidates:
        stem = f"{_safe_stem(entry.name)}#id{entry.decoded_identifier}"
        output = (
            args.output / f"{stem}.glb"
            if args.format == "glb"
            else args.output / stem / f"{stem}.gltf"
        )
        item: dict[str, Any] = {
            "name": entry.name,
            "resourceKey": f"0x{entry.resource_key:08X}",
            "decodedId": entry.decoded_identifier,
            "output": str(output.relative_to(args.output)),
            "warnings": [],
            "errors": [],
        }
        try:
            result = export_hob(
                archive,
                entry,
                output,
                output_format=args.format,
                skip_unsupported=args.skip_unsupported,
                native_coordinates=args.native_coordinates,
            )
            item["status"] = "succeeded"
            item["stats"] = result.stats.as_dict()
            item["warnings"] = result.warnings
            report["summary"]["succeeded"] += 1
        except ConversionError as error:
            item["status"] = "failed"
            item["errors"].append(error.as_report())
            report["summary"]["failed"] += 1
            print(error, file=sys.stderr)
        except Exception as error:  # pragma: no cover - last-resort model isolation
            item["status"] = "failed"
            item["errors"].append({"message": f"internal error: {error}"})
            report["summary"]["failed"] += 1
            print(f"{entry.name}: internal error: {error}", file=sys.stderr)
        report["models"].append(item)

    _write_report(args.output, report)
    return 1 if report["summary"]["failed"] else 0


def _select(entries: list[HgarEntry], selector: str) -> list[HgarEntry]:
    by_name = [
        entry
        for entry in entries
        if selector.casefold() in {entry.name.casefold(), entry.short_name.casefold()}
    ]
    if by_name:
        return by_name
    try:
        value = int(selector, 0)
    except ValueError:
        return []
    return [
        entry
        for entry in entries
        if value in (entry.decoded_identifier, entry.resource_key, entry.encoded_identifier)
    ]


def _write_report(output: Path, report: dict[str, Any]) -> None:
    output.mkdir(parents=True, exist_ok=True)
    path = output / "conversion-report.json"
    fd, name = tempfile.mkstemp(prefix=".conversion-report.", suffix=".tmp", dir=output)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as stream:
            json.dump(report, stream, indent=2, ensure_ascii=True)
            stream.write("\n")
        os.replace(name, path)
    except Exception:
        Path(name).unlink(missing_ok=True)
        raise


def _safe_stem(name: str) -> str:
    value = re.sub(r"[^A-Za-z0-9._-]+", "_", Path(name).stem).strip("._")
    return value or "model"
