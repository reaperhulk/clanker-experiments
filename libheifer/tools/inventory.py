#!/usr/bin/env python3
"""Inventory the pinned headers independently of the candidate implementation.

Requires the pinned `libclang==18.1.1` Python package and a C compiler's headers.
This extracts declarations, not just strings matching a function naming pattern.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile

from clang import cindex

PIN = "4e14f5942c1732ace9611b9522cc991501445463"


def inventory(source):
    source = Path(source).resolve()
    revision = subprocess.check_output(["git", "-C", str(source), "rev-parse", "HEAD"], text=True).strip()
    if revision != PIN:
        raise SystemExit(f"Reference mismatch: {revision} != {PIN}")
    include = source / "libheif/api"
    headers = include / "libheif"
    cmake = (source / "libheif/CMakeLists.txt").read_text()
    installed = set(re.findall(r"api/libheif/(\w+\.h)", cmake.split("set(libheif_sources")[0]))
    hashes, groups, functions, records, enums, typedefs, variables, macros = {}, {}, {}, {}, {}, {}, {}, {}
    for header in sorted(headers.glob("*.h")):
        if header.name in ("heif_cxx.h", "heif_emscripten.h"):
            group = "cxx-consumer"
        elif header.name in installed:
            group = "installed"
        else:
            group = "non-installed"
        groups[header.name] = group
        hashes[header.name] = hashlib.sha256(header.read_bytes()).hexdigest()
    gcc_headers = subprocess.check_output(["cc", "-print-file-name=include"], text=True).strip()
    with tempfile.TemporaryDirectory() as tmp:
        version = Path(tmp) / "libheif"
        version.mkdir()
        text = (headers / "heif_version.h.in").read_text()
        for key, value in {"PROJECT_VERSION_MAJOR": "1", "PROJECT_VERSION_MINOR": "23", "PROJECT_VERSION_PATCH": "4", "PLUGIN_DIRECTORY": ""}.items():
            text = text.replace(f"@{key}@", value)
        (version / "heif_version.h").write_text(text)
        # Parse all C headers, not just the umbrella header. This also captures
        # experimental APIs so they cannot disappear unnoticed from the scope.
        text = "\n".join(f"#include <libheif/{name}>" for name in groups if groups[name] != "cxx-consumer")
        unit = cindex.Index.create().parse("inventory.c", args=["-x", "c", "-std=c11", f"-I{include}", f"-I{tmp}", f"-I{gcc_headers}"], unsaved_files=[("inventory.c", text)], options=cindex.TranslationUnit.PARSE_DETAILED_PROCESSING_RECORD)
        errors = [str(d) for d in unit.diagnostics if d.severity >= cindex.Diagnostic.Error]
        if errors:
            raise SystemExit("\n".join(errors))
        for cursor in unit.cursor.walk_preorder():
            if not cursor.location.file or Path(cursor.location.file.name).parent != headers:
                continue
            name = cursor.spelling
            if not name.startswith(("heif_", "LIBHEIF_")):
                continue
            header = Path(cursor.location.file.name).name
            common = {"header": header, "scope": groups[header]}
            kind = cursor.kind
            if kind == cindex.CursorKind.MACRO_DEFINITION:
                macros[name] = dict(common, tokens=[t.spelling for t in cursor.get_tokens()][1:])
            elif kind == cindex.CursorKind.FUNCTION_DECL:
                functions[name] = dict(common, result=cursor.result_type.spelling, parameters=[p.type.spelling for p in cursor.get_arguments()], variadic=cursor.type.is_function_variadic())
            elif kind == cindex.CursorKind.STRUCT_DECL and cursor.is_definition():
                records[name] = dict(common, fields=[{"name": f.spelling, "type": f.type.spelling} for f in cursor.get_children() if f.kind == cindex.CursorKind.FIELD_DECL])
            elif kind == cindex.CursorKind.ENUM_DECL and cursor.is_definition():
                enums[name] = dict(common, values={e.spelling: e.enum_value for e in cursor.get_children() if e.kind == cindex.CursorKind.ENUM_CONSTANT_DECL})
            elif kind == cindex.CursorKind.TYPEDEF_DECL:
                typedefs[name] = dict(common, type=cursor.underlying_typedef_type.spelling)
            elif kind == cindex.CursorKind.VAR_DECL and cursor.storage_class == cindex.StorageClass.EXTERN:
                # Some plugin-header externs are internal conveniences with no
                # export annotation. Keep them in the inventory but distinguish
                # the actual shared-library ABI contract.
                source_text = Path(cursor.location.file.name).read_text()
                prefix = source_text[source_text.rfind(";", 0, cursor.extent.start.offset) + 1:cursor.location.offset]
                variables[name] = dict(common, type=cursor.type.spelling, exported="LIBHEIF_API" in prefix)
    return {"reference": {"version": "1.23.4", "commit": PIN}, "headers": {n: {"sha256": hashes[n], "scope": groups[n]} for n in hashes}, "functions": dict(sorted(functions.items())), "variables": dict(sorted(variables.items())), "structs": dict(sorted(records.items())), "enums": dict(sorted(enums.items())), "typedefs": dict(sorted(typedefs.items())), "macros": dict(sorted(macros.items()))}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", default="tests/upstream")
    parser.add_argument("--output", default="compat/api.json")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    data = inventory(args.source)
    output = json.dumps(data, indent=2, sort_keys=True) + "\n"
    if args.check:
        if Path(args.output).read_text() != output:
            raise SystemExit("API inventory drift: regenerate and review the contract")
    else:
        Path(args.output).write_text(output)
    counts = {scope: sum(f["scope"] == scope for f in data["functions"].values()) for scope in ("installed", "non-installed")}
    print(json.dumps({"functions": counts, "structs": len(data["structs"]), "enums": len(data["enums"]), "headers": len(data["headers"])}))


if __name__ == "__main__":
    main()
