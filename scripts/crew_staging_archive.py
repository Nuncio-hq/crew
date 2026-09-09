"""Bound local archive creation before writes, including headers and padding."""

import os
import stat
import tarfile
import time

from crew_staging_config import require


class BoundedWriter:
    def __init__(self, stream, limit, deadline):
        self.stream, self.limit, self.deadline, self.size = stream, limit, deadline, 0

    def write(self, data):
        require(time.monotonic() < self.deadline, "asset capture deadline exceeded")
        require(self.size + len(data) <= self.limit, "snapshot archive size limit exceeded")
        self.size += len(data)
        return self.stream.write(data)

    def tell(self):
        return self.size


class TimedReader:
    def __init__(self, stream, deadline):
        self.stream, self.deadline = stream, deadline

    def read(self, size):
        require(time.monotonic() < self.deadline, "asset capture deadline exceeded")
        return self.stream.read(size)


def capture_local(root, stream, limits):
    deadline = time.monotonic() + limits["timeout_seconds"]
    writer = BoundedWriter(stream, limits["snapshot_bytes"], deadline)
    count = 0
    with tarfile.open(fileobj=writer, mode="w", format=tarfile.PAX_FORMAT) as archive:
        def add(item):
            nonlocal count
            count += 1
            require(count <= 10000 and time.monotonic() < deadline, "asset count or deadline exceeded")
            info = item.lstat()
            require(stat.S_ISDIR(info.st_mode) or stat.S_ISREG(info.st_mode) and info.st_nlink == 1,
                    "local asset symlink, hardlink or special file refused")
            require(info.st_size <= limits["snapshot_bytes"], "snapshot asset size limit exceeded")
            member = archive.gettarinfo(str(item), arcname=str(item.relative_to(root.parent)))
            if member.isdir():
                archive.addfile(member)
                with os.scandir(item) as entries:
                    for entry in entries:
                        add(item / entry.name)
            else:
                # O_NOFOLLOW closes the path replacement race between lstat/open.
                descriptor = os.open(item, os.O_RDONLY | os.O_NOFOLLOW)
                with os.fdopen(descriptor, "rb") as source:
                    current = os.fstat(source.fileno())
                    require((current.st_dev, current.st_ino, current.st_size, current.st_nlink)
                            == (info.st_dev, info.st_ino, info.st_size, 1), "asset changed during open")
                    archive.addfile(member, TimedReader(source, deadline))
        add(root)
