#!/usr/bin/env python3
"""Compact-container file input and retained metadata after unlink and context release."""
import sys
import test_file_input
from test_mini import corpus

if __name__ == '__main__':
    test_file_input.corpus = corpus
    test_file_input.__doc__ = __doc__
    if '--work' not in sys.argv: sys.argv += ['--work', '.build/mini-file']
    if '--output' not in sys.argv: sys.argv += ['--output', '.build/mini-file-report.json']
    test_file_input.main()
