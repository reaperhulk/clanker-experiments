#!/usr/bin/env python3
"""Independent compact-container property ordering and HDR metadata comparisons."""
import sys
import test_context
from test_mini import corpus

if __name__ == '__main__':
    test_context.corpus = corpus
    test_context.CLIENT = 'tests/mini_properties.c'
    test_context.SCOPE = __doc__
    if '--output' not in sys.argv: sys.argv += ['--output', '.build/mini-properties-report.json']
    test_context.main()
