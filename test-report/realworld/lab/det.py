"""Deterministic byte stream shared by targets and clients, so both ends can verify every byte."""
import random
import struct

BLK = 1 << 16
_base = {}


def _b(seed):
    b = _base.get(seed)
    if b is None:
        if len(_base) > 64:
            _base.clear()
        b = random.Random(seed).randbytes(BLK - 16)
        _base[seed] = b
    return b


def block(seed, i):
    return struct.pack(">QQ", seed, i) + _b(seed)


def stream(seed, n):
    i = 0
    while n > 0:
        b = block(seed, i)
        if n < BLK:
            b = b[:n]
        yield b
        n -= len(b)
        i += 1


class Checker:
    """Feed received bytes, remembers the first offset that differs from the expected stream."""

    def __init__(self, seed):
        self.seed, self.off, self.bad = seed, 0, None

    def feed(self, data):
        mv = memoryview(data)
        pos = 0
        while pos < len(mv):
            bi, bo = divmod(self.off, BLK)
            n = min(len(mv) - pos, BLK - bo)
            if self.bad is None and mv[pos:pos + n] != block(self.seed, bi)[bo:bo + n]:
                self.bad = self.off
            pos += n
            self.off += n
