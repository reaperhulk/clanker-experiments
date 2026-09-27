#!/usr/bin/env python3
"""Selected independent JPEG damaged streams for mutation sensitivity; full prefixes have a separate gate."""
import sys
import test_jpeg_errors
if __name__=='__main__':
    if '--work' not in sys.argv:sys.argv+=['--work','.build/jpeg-recovery']
    if '--output' not in sys.argv:sys.argv+=['--output','.build/jpeg-recovery-report.json']
    test_jpeg_errors.main(all_prefixes=False)
