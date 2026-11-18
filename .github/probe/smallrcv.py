"""How much a loopback connection takes before a write blocks for 200 ms, when the never-reading peer's receive
buffer is set small on the listener before it accepts."""
import select, socket, sys, time

def trial(rcvbuf):
    server = socket.socket()
    if rcvbuf:
        server.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, rcvbuf)
    server.bind(("127.0.0.1", 0)); server.listen(1)
    client = socket.create_connection(server.getsockname()); peer, _ = server.accept()
    client.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1); client.setblocking(False)
    chunk = b"x" * 16384; total = 0
    while True:
        _, w, _ = select.select([], [client], [], 0.2 if total < 1 << 20 else 1.0)
        if not w:
            break
        try:
            total += client.send(chunk)
        except BlockingIOError:
            pass
        if total > 1 << 30:
            break
    print(f"{sys.platform} peer rcvbuf asked {rcvbuf or 'default'}, got {peer.getsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF)}: {total / 1024:.0f} KB before blocking")
    client.close(); peer.close(); server.close()

for rcvbuf in [0, 0, 4096, 4096, 4096, 16384, 16384]:
    trial(rcvbuf)
