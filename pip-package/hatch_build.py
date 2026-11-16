"""Hatchling build hook that turns the wheel into a platform wheel.

``build_wheels.py`` invokes the backend once per release target with ``ADRLINT_WHEEL_TAG`` and
``ADRLINT_WHEEL_BINARY`` set; this hook stamps the wheel with that platform tag and embeds the
binary under ``adrlint/bin/``. With neither set it builds the pure-Python ``py3-none-any`` fallback
wheel, which carries no binary, downloads the release archive at first run, and therefore depends on
certifi.
"""

from __future__ import annotations

import os
from pathlib import Path
from typing import Any

from hatchling.builders.hooks.plugin.interface import BuildHookInterface

# Only the fallback wheel downloads anything, so only it needs a CA bundle.
FALLBACK_DEPENDENCIES = ["certifi>=2026.10.23"]


class CustomBuildHook(BuildHookInterface):
    """Stamp the wheel with a platform tag and embed the prebuilt binary."""

    PLUGIN_NAME = "custom"

    def initialize(self, version: str, build_data: dict[str, Any]) -> None:  # noqa: ARG002
        tag = os.environ.get("ADRLINT_WHEEL_TAG")
        binary = os.environ.get("ADRLINT_WHEEL_BINARY")

        if not tag and not binary:
            build_data["dependencies"].extend(FALLBACK_DEPENDENCIES)
            return

        if not (tag and binary):
            message = "ADRLINT_WHEEL_TAG and ADRLINT_WHEEL_BINARY must be set together"
            raise ValueError(message)

        if not Path(binary).is_file():
            message = f"ADRLINT_WHEEL_BINARY does not exist: {binary}"
            raise FileNotFoundError(message)

        target_name = "adrlint.exe" if tag.startswith("win") else "adrlint"

        build_data["pure_python"] = False
        build_data["infer_tag"] = False
        build_data["tag"] = f"py3-none-{tag}"
        build_data["force_include"][binary] = f"adrlint/bin/{target_name}"
