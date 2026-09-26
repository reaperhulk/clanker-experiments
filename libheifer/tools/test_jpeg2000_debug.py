#!/usr/bin/env python3
"""Original-header exact JPEG2000 nested property diagnostics."""
import sys
import test_debug_dump
from test_jpeg2000_properties import corpus
if __name__=='__main__':
 if '--work' not in sys.argv:sys.argv+=['--work','.build/jpeg2000-debug']
 if '--output' not in sys.argv:sys.argv+=['--output','.build/jpeg2000-debug-report.json']
 test_debug_dump.cases=lambda:corpus()[0];test_debug_dump.__doc__=__doc__;test_debug_dump.main()
