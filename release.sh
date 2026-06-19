#!/usr/bin/env bash

set -x
set -e
set -u
set -o pipefail

zig version

# Github actions requires to mark the current git repository as safe
git config --global --add safe.directory "$(pwd)"
