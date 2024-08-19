#!/usr/bin/env python3
# Licensed to the Apache Software Foundation (ASF) under one
# or more contributor license agreements.  See the NOTICE file
# distributed with this work for additional information
# regarding copyright ownership.  The ASF licenses this file
# to you under the Apache License, Version 2.0 (the
# "License"); you may not use this file except in compliance
# with the License.  You may obtain a copy of the License at
#
#   http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing,
# software distributed under the License is distributed on an
# "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
# KIND, either express or implied.  See the License for the
# specific language governing permissions and limitations
# under the License.

import subprocess
import sys
import platform
from pathlib import Path
from constants import PACKAGES


if __name__ == "__main__":
    command_line_args = sys.argv[1:]
    command = " ".join(command_line_args)

    for directory in PACKAGES:
        print(f"Executing '{command}' in {directory}")

        # Don't build cloud filter on non-Windows platform
        if (
            command.startswith("cargo")
            and platform.system().lower() != "windows"
            and f"{directory}" == "integrations/cloudfilter"
        ):
            print(f"Skip {directory} because it only builds on Windows")
            continue

        # Make cargo happy if `Cargo.toml` not exist
        if (
            command.startswith("cargo")
            and not (Path(directory) / "Cargo.toml").exists()
        ):
            print(f"Skip {directory} because `Cargo.toml` not exist")
            continue

        subprocess.run(
            command,
            shell=True,
            cwd=directory,
            check=True,
        )
