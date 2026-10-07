#!/usr/bin/env bash
# Sert dist/ sur http://127.0.0.1:8080
cd "$(dirname "$0")/../dist" && exec python3 -m http.server 8080 --bind 127.0.0.1
