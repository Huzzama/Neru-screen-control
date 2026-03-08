#!/usr/bin/env python3
"""
TRCC entry point.

Usage:
    python main.py
    python main.py --ui
    python main.py --config config.json
    python main.py --ui --config config.json
"""

import os
import sys

ROOT_DIR = os.path.dirname(os.path.abspath(__file__))
SRC_DIR = os.path.join(ROOT_DIR, "src")

if SRC_DIR not in sys.path:
    sys.path.insert(0, SRC_DIR)

from controller import main

if __name__ == "__main__":
    main()