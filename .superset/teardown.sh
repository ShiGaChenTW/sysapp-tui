#!/usr/bin/env bash
#
# Superset adapter — 薄包裝，呼叫工具中立的 teardown
#
exec bash "$(dirname "$0")/../scripts/teardown.sh"
