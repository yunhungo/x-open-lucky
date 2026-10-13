#!/usr/bin/env python3
# @created 2026-10-13
# @description 用有界回环 UDP 转发模拟 QUIC 往返延迟，不修改宿主网络或生产流量。
# @author yunhungo
import heapq
import select
import socket
import threading
import time


class QuicLatency:
    def __init__(self, port, rtt_ms):
        self.upstream = ('127.0.0.1', port)
        self.delay = rtt_ms / 2000
        self.socket = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.socket.bind(('127.0.0.1', 0))
        self.socket.setblocking(False)
        self.port = self.socket.getsockname()[1]
        self.dropped = 0
        self.stop = threading.Event()
        self.worker = threading.Thread(target=self.run, daemon=True)
        self.worker.start()

    def run(self):
        pending = []
        peer = None
        sequence = 0
        while not self.stop.is_set():
            now = time.monotonic()
            while pending and pending[0][0] <= now:
                _, _, data, destination = heapq.heappop(pending)
                try:
                    self.socket.sendto(data, destination)
                except OSError:
                    self.dropped += 1
            timeout = min(.02, max(0, pending[0][0] - now)) if pending else .02
            if not select.select([self.socket], [], [], timeout)[0]:
                continue
            # 每轮限制接收数量，让到期的数据包及时发出。
            for _ in range(64):
                try:
                    data, source = self.socket.recvfrom(65536)
                except BlockingIOError:
                    break
                if source == self.upstream:
                    destination = peer
                else:
                    if peer is None:
                        peer = source
                    if source != peer:
                        self.dropped += 1
                        continue
                    destination = self.upstream
                if destination is None or len(pending) >= 4096:
                    self.dropped += 1
                    continue
                sequence += 1
                heapq.heappush(pending, (time.monotonic() + self.delay, sequence, data, destination))

    def close(self):
        self.stop.set()
        self.worker.join()
        self.socket.close()
