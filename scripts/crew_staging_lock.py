"""Serialize operations sharing staging resources without waiting indefinitely."""

from contextlib import contextmanager
import fcntl
import os

from crew_staging_config import Refused, require


@contextmanager
def operation_lock(root):
    descriptor = os.open(root / '.crew-staging.lock', os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
    try:
        info = os.fstat(descriptor)
        require(info.st_uid == os.getuid() and info.st_nlink == 1 and info.st_mode & 0o077 == 0,
                'operation lock must be private and owned')
        try:
            fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise Refused('staging operation already running; retry after its recorded completion') from None
        yield
    finally:
        os.close(descriptor)
