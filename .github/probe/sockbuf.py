"""How much a loopback TCP connection takes before a write blocks, when the peer never reads."""
import select, socket, subprocess, sys, time

for name in ["net.inet.tcp.autosndbufmax", "net.inet.tcp.autorcvbufmax", "net.inet.tcp.sendspace",
             "net.inet.tcp.recvspace", "kern.ipc.maxsockbuf", "net.inet.tcp.doautosndbuf", "net.inet.tcp.doautorcvbuf"]:
    out = subprocess.run(["sysctl", "-n", name], capture_output=True, text=True)
    print(name, out.stdout.strip() or out.stderr.strip())

def trial(stall, pause):
    server = socket.socket(); server.bind(("127.0.0.1", 0)); server.listen(1)
    client = socket.create_connection(server.getsockname()); peer, _ = server.accept()
    client.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
    client.setblocking(False)
    chunk = b"x" * 65536; total = 0; start = time.monotonic()
    while True:
        _, writable, _ = select.select([], [client], [], stall)
        if not writable:
            break
        try:
            total += client.send(chunk)
        except BlockingIOError:
            continue
        if pause:
            time.sleep(pause)
        if total > 1 << 30:
            break
    print(f"stall {stall}s pause {pause}s: {total/1048576:.1f} MB in {time.monotonic()-start:.2f}s, "
          f"sndbuf {client.getsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF)} rcvbuf {peer.getsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF)}")
    client.close(); peer.close(); server.close()

for stall, pause in [(0.2, 0), (0.2, 0), (0.2, 0), (0.2, 0.001), (1.0, 0)]:
    trial(stall, pause)
