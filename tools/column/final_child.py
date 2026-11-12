"""Do not execute a producer until its process group is durably registered."""
import os
import sys

fd=int(sys.argv[1])
try: ready=os.read(fd,1)
finally: os.close(fd)
if ready!=b'!': raise SystemExit(125)
os.execvpe(sys.argv[2],sys.argv[2:],os.environ)
