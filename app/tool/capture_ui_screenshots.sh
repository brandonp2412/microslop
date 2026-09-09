#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
flutter test test/ui_screenshot_test.dart
find build/ui-screenshots -maxdepth 1 -type f -name '*.png' -printf '%f\n' | sort
